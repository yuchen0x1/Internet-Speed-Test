// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

#[cfg(feature = "semver")]
use crate::semver_compat::semver_compat_string;

use crate::{SingleInstanceCallback, manage_instance_lock};
use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Resource, Runtime,
    plugin::{self, TauriPlugin},
};
use zbus::{Connection, interface, names::WellKnownName};

/// Owns the D-Bus name, released when the app exits (see [`crate::destroy`]).
struct InstanceLock {
    connection: Connection,
    name: String,
}

impl Resource for InstanceLock {}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if let Ok(name) = WellKnownName::try_from(self.name.as_str()) {
            // Drop may run from a Tauri async command. Do not enter zbus's
            // synchronous executor from that runtime's worker thread.
            std::thread::scope(|scope| {
                let _ = scope
                    .spawn(|| zbus::block_on(self.connection.release_name(name)))
                    .join();
            });
        }
    }
}

struct SingleInstanceDBus<R: Runtime> {
    callback: Arc<Mutex<Box<SingleInstanceCallback<R>>>>,
    app_handle: AppHandle<R>,
}

#[interface(name = "org.SingleInstance.DBus")]
impl<R: Runtime> SingleInstanceDBus<R> {
    async fn execute_callback(&self, argv: Vec<String>, cwd: String) -> zbus::fdo::Result<()> {
        let callback = Arc::clone(&self.callback);
        let app = self.app_handle.clone();
        // A callback may release the instance lock. Await it off the D-Bus
        // executor so release_name can still receive its reply on that executor.
        tauri::async_runtime::spawn_blocking(move || {
            let mut callback = callback
                .lock()
                .map_err(|_| "single-instance callback lock was poisoned".to_string())?;
            (callback)(&app, argv, cwd);
            Ok::<_, String>(())
        })
        .await
        .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?
        .map_err(zbus::fdo::Error::Failed)
    }
}

pub fn init<R: Runtime>(
    callback: Box<SingleInstanceCallback<R>>,
    dbus_id: Option<String>,
) -> TauriPlugin<R> {
    plugin::Builder::new("single-instance")
        .setup(move |app, _api| zbus::block_on(async move {
            let mut dbus_name = dbus_id.unwrap_or_else(|| app.config().identifier.clone());
            dbus_name.push_str(".SingleInstance");

            #[cfg(feature = "semver")]
            {
                dbus_name.push('_');
                dbus_name.push_str(semver_compat_string(&app.package_info().version).as_str());
            }

            let mut dbus_path = dbus_name.replace('.', "/").replace('-', "_");
            if !dbus_path.starts_with('/') {
                dbus_path = format!("/{dbus_path}");
            }

            let single_instance_dbus = SingleInstanceDBus {
                callback: Arc::new(Mutex::new(callback)),
                app_handle: app.clone(),
            };

            // zbus 6 defers builder errors until build; keep invalid application IDs
            // distinct from an unavailable session bus, which must allow startup.
            let name = WellKnownName::try_from(dbus_name.as_str())?;
            let path = zbus::ObjectPath::try_from(dbus_path.as_str())?;
            match zbus::connection::Builder::session()
                .name(name)
                .replace_existing_names(false)
                .allow_name_replacements(false)
                .serve_at(path, single_instance_dbus)
                .build()
                .await
            {
                Ok(connection) => {
                    manage_instance_lock(
                        app,
                        InstanceLock {
                            connection,
                            name: dbus_name,
                        },
                    );
                }
                Err(zbus::Error::NameTaken) => {
                    let connection = Connection::session().await?;
                    connection
                        .call_method(
                            Some(dbus_name.as_str()),
                            dbus_path.as_str(),
                            Some("org.SingleInstance.DBus"),
                            "ExecuteCallback",
                            &(
                                std::env::args_os()
                                    .map(|arg| arg.to_string_lossy().into_owned())
                                    .collect::<Vec<String>>(),
                                std::env::current_dir()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .as_ref(),
                            ),
                        )
                        .await
                        .map_err(|error| {
                            format!("failed to forward arguments to the running instance: {error}")
                        })?;
                    // Exit successfully only after the primary accepted the arguments.
                    app.cleanup_before_exit();
                    std::process::exit(0);
                }
                Err(error) => {
                    // Without a session bus there is no way to find other instances.
                    tracing::warn!(
                        "single-instance: D-Bus session bus unavailable, launching normally: {error}"
                    );
                    return Ok(());
                }
            }

            Ok(())
        }))
        .build()
}
