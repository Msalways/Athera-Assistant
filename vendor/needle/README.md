# Needle native integration

Pinned model repository: https://huggingface.co/Cactus-Compute/needle2/tree/32e9e3a93b205f786929697446ae669cf0a84579

Inspected 2026-09-10: the Android ARM64 distribution contains `libneedle.a`, `needle`, and `needle.h`. It is a static archive, not an assumed shared library. The published license is Apache-2.0; redistribute its license and applicable notices with any bundled binary.

The header declares `needle_init(const char*, const char*, const char*) -> int`, `needle_complete(const char*, int, char*, int) -> int`, `needle_reset()`, and `needle_load(const unsigned char*, unsigned long long) -> int`.

`provider-needle` supports the verified Needle 2 ABI only. Needle 3 has a different completion signature and must not be loaded through these bindings. The static feature requires `NEEDLE_LIB_DIR`; ordinary builds report Needle unavailable and exercise configured cloud fallback. Missing binaries are never replaced by fake inference.

Native calls run on a blocking worker behind a process-wide mutex. A timeout discards the result but cannot forcibly interrupt native code. Native code does not execute tools, so a late inference cannot execute an action. Physical ARM64 latency, archive linking, memory usage, and lifecycle validation remain release gates.

Reference: https://github.com/cactus-compute/needle/blob/main/needle/__init__.py
