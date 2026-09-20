---
name: assistant-eval-engineering
description: Add regression coverage and measure this assistant's real Rust execution and model routing.
---
Use the actual Assistant core from the JSON-lines runner. Keep fixture results distinct from live model measurements. Cover approvals, disabled capabilities, changed schemas, auth resume, interrupted writes and provider fallback with exact assertions. Report measured latency and accuracy only; physical-device acceptance cannot be inferred from host tests.
