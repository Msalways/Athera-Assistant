# Local chat native runtime

`llama.cpp` is fetched at release tag `b10886` (commit
`f1b6fbf35cfa010b0a8d6301fdfccbb7f41bd903`) and built locally; source and
binaries are not committed. The model is the `ggml-org` Q4_K_M conversion of
Qwen3-1.7B at revision `daeb8e2d528a760970442092f6bf1e55c3b659eb`.

- Runtime: https://github.com/ggml-org/llama.cpp/releases/tag/b10886
- Model: https://huggingface.co/ggml-org/Qwen3-1.7B-GGUF/blob/daeb8e2d528a760970442092f6bf1e55c3b659eb/Qwen3-1.7B-Q4_K_M.gguf
- Model license: Apache-2.0
- Model bytes: `1282439264`
- Model SHA-256: `d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5`

Run `scripts/fetch-local-chat.ps1`. The script checks out the exact runtime
tag and builds the host `llama-cli` feasibility adapter plus
`athera_local_chat`. Pass `-Android` with `ANDROID_NDK_HOME` set to build only
the ARM64 shared library for Android API 31. Model downloads happen through
`ModelManager` so the app can report progress and cancel safely.
