// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Fabian Schmieder

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=webui/src");
    println!("cargo:rerun-if-changed=webui/messages");
    println!("cargo:rerun-if-changed=webui/public");
    println!("cargo:rerun-if-changed=webui/package.json");
    println!("cargo:rerun-if-changed=webui/next.config.ts");

    let webui_dir = std::path::Path::new("webui");

    // Install deps if needed
    if !webui_dir.join("node_modules").exists() {
        let status = Command::new("npm")
            .args(["ci", "--prefer-offline"])
            .current_dir(webui_dir)
            .status()
            .expect("failed to run npm ci — is Node.js installed?");
        assert!(status.success(), "npm ci failed");
    }

    // Build static export
    let status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(webui_dir)
        .status()
        .expect("failed to run npm run build");
    assert!(status.success(), "webui build failed");
}
