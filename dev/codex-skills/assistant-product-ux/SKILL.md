---
name: assistant-product-ux
description: Implement conversation, execution, approval, and recovery states for this assistant.
---
Render the Rust task status explicitly. Show exact arguments and action-specific confirmation for writes. Distinguish unavailable providers, auth pauses, approval pauses, and uncertain action outcomes. Never label scripted fixtures as real inference. Voice capture is deferred; do not render a working microphone control until the speech adapter exists.
