---
name: tauri-mobile-architecture
description: Maintain this app's Tauri IPC, Rust orchestration, and Android integration boundaries.
---
React is presentation only. Keep IPC handlers thin and share the real Assistant runtime with evaluation tooling. Vendor objects stay in adapters. Kotlin owns Android permissions and Keystore integration. Never place credentials in frontend persistence or model packets. Do not ship Python with the app.
