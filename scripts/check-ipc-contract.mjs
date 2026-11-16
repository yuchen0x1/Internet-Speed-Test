#!/usr/bin/env node

/**
 * Keep the three IPC surfaces aligned without requiring a running Tauri window:
 * React's invoke calls, Rust command registration, and Rust command definitions.
 *
 * This intentionally uses the source files as the contract input. It catches a
 * renamed/removed command before the frontend can ship a runtime "command not
 * found" error, while avoiding a generated manifest or a new runtime dependency.
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const apiPath = path.join(root, "src", "api.ts");
const libPath = path.join(root, "src-tauri", "src", "lib.rs");
const commandsPath = path.join(root, "src-tauri", "src", "commands.rs");

const read = (file) => fs.readFileSync(file, "utf8");
const uniqueSorted = (values) => [...new Set(values)].sort();
const quotedInvoke = /\binvoke(?:<[^>\n]+>)?\s*\(\s*["']([^"']+)["']/g;

const apiSource = read(apiPath);
const libSource = read(libPath);
const commandsSource = read(commandsPath);

const apiCommands = uniqueSorted([...apiSource.matchAll(quotedInvoke)].map((match) => match[1]));

const handlerStart = libSource.indexOf(".invoke_handler(tauri::generate_handler![");
if (handlerStart < 0) {
  throw new Error("Could not find the Tauri invoke_handler command registry in src-tauri/src/lib.rs");
}
const handlerTail = libSource.slice(handlerStart);
const handlerEndOffset = handlerTail.search(/\]\)\s*\.run\(/);
if (handlerEndOffset < 0) {
  throw new Error("Could not find the end of the Tauri invoke_handler command registry in src-tauri/src/lib.rs");
}
const handlerSource = handlerTail.slice(0, handlerEndOffset);
const registeredCommands = uniqueSorted(
  [...handlerSource.matchAll(/commands::([A-Za-z0-9_]+)/g)].map((match) => match[1]),
);

// A command may have clippy/doc attributes between #[tauri::command] and fn.
const commandDefinition = /#\[tauri::command\][\s\S]{0,500}?\bpub\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)/g;
const definedCommands = uniqueSorted(
  [...commandsSource.matchAll(commandDefinition)].map((match) => match[1]),
);

const difference = (left, right) => left.filter((value) => !right.includes(value));
const apiOnly = difference(apiCommands, registeredCommands);
const registeredOnly = difference(registeredCommands, apiCommands);
const registryWithoutDefinition = difference(registeredCommands, definedCommands);

const report = {
  api_commands: apiCommands.length,
  registered_commands: registeredCommands.length,
  defined_commands: definedCommands.length,
  api_only: apiOnly,
  registered_only: registeredOnly,
  registry_without_definition: registryWithoutDefinition,
};

if (process.argv.includes("--json")) {
  console.log(JSON.stringify(report, null, 2));
}

if (apiOnly.length || registeredOnly.length || registryWithoutDefinition.length) {
  if (!process.argv.includes("--json")) {
    console.error(JSON.stringify(report, null, 2));
  }
  process.exitCode = 1;
} else if (!process.argv.includes("--json")) {
  console.log(
    `IPC contract OK: ${apiCommands.length} frontend commands, ${registeredCommands.length} registered commands, ${definedCommands.length} Rust command definitions`,
  );
}
