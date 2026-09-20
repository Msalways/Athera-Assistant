# On-Device Reasoning Model R&D

## Executive decision

AETHRA can become materially better at reasoning on a phone, but the improvement will
not come from adding a few layers to the existing 14 MB Needle binary. Needle is a
specialized tool caller. Fine-tuning can improve its tool selection, argument
grounding and abstention for AETHRA's tools; it cannot turn a 45M-parameter model into
a general conversational reasoner.

The strongest research direction is a local cascade:

1. Keep deterministic policy, memory and execution in Rust.
2. Benchmark an upgraded Needle 2 and a fine-tuned FunctionGemma 270M as competing
   local routers.
3. Add one downloadable, on-demand local reasoner in the 0.6B-1.2B range.
4. Unload that reasoner after the task or under Android memory pressure.
5. Use a cloud model only when the user permits it and the local reasoner is
   insufficient.

The first local-reasoner candidates should be Falcon-H1-Tiny-R-0.6B and
LFM2.5-1.2B-Thinking. Qwen3.5-0.8B is a useful multimodal control candidate. Native
1.58-bit BitNet models merit an isolated benchmark branch, but the current evidence
does not justify making BitNet the production runtime.

This recommendation deliberately separates three questions:

- **Can a small model select phone tools?** Yes, with task-specific training and
  constrained schemas.
- **Can a small model perform limited multi-step reasoning?** Increasingly yes in the
  0.6B-1.2B range, with meaningful limits.
- **Can a sub-1 GB local model match Claude or Codex generally?** No available evidence
  supports that claim.

## Current AETHRA baseline

The current integration pins a specific Needle 2 Android archive and its C ABI. The
Rust provider exposes a compact working packet, requires exactly one function call and
does not use upstream confidence in routing. It correctly keeps native inference away
from policy and execution.

The real host results are weak enough that a model-development experiment is
warranted. Of six recorded SMS cases, argument extraction passed once, rewriting
returned the original wording, and four cases returned an invalid response. These are
only six cases and are not a statistically useful benchmark, but they show that the
current build has not demonstrated general routing reliability. See
[`artifacts/needle-sms-host.jsonl`](../artifacts/needle-sms-host.jsonl).

Upstream Needle 2 is now described as a 45M-parameter, 14 MB tool-calling model with a
roughly 28 MB session, grammar-constrained calls, a learned confidence output and
top-five tool retrieval.[^1] Those capabilities are useful, but AETHRA must verify
that the pinned Android C archive exposes the same behavior. The upstream package and
ABI have changed quickly; replacing the archive without a versioned compatibility
test would repeat the integration failures already observed.

The target device identified in the experiment is a OnePlus CPH2707, corresponding to
the OnePlus Nord 5 family with Snapdragon 8s Gen 3.[^2] Qualcomm advertises the SoC as
supporting on-device generative models, including INT4 acceleration, but that is a
hardware capability statement rather than proof that an ordinary sideloaded app can
use the NPU efficiently.[^3] CPU inference is therefore the portable baseline; GPU or
NPU acceleration should be treated as a measured optimization.

## What “improving the reasoning layers” can mean

There are four different interventions, with very different cost and expected value.

### Fine-tune the existing Needle weights

Needle's supported fine-tuning path applies LoRA adapters to attention projections and
merges them into a new `.cact` archive. Its documentation says a few hundred clean
examples can move tool selection, while argument grounding generally needs thousands
of varied examples.[^4]

This can teach behavior such as:

- `contacts.search` instead of `sms.open_composer` when a person is unresolved;
- asking for clarification when several contacts match;
- extracting exact times, phone numbers and message spans;
- choosing `request_assistance` for synthesis or unfamiliar tasks;
- selecting the next screen operation from a bounded observation.

It cannot add network depth, expand the base model's language capacity or create a
large working memory. It modifies how the existing capacity is used. Upstream also
states that its calibrated confidence head is not updated by fine-tuning, so tuned
weights return no calibrated confidence.[^5] AETHRA would need held-out calibration
and deterministic acceptance rules rather than trusting a missing score.

There is a further release risk: an open upstream report describes a Needle LoRA that
worked before export but changed decision behavior after 2-bit and 4-bit export.[^6]
This does not prove every fine-tune is broken, but it requires float-versus-exported
regression tests before a tuned archive reaches the APK.

### Distill task behavior from a stronger teacher

Knowledge distillation trains a small model on high-quality demonstrations from a
larger model or humans. DeepSeek's published R1 work found that reasoning traces from
a larger model improved smaller dense models more than asking small models to discover
all reasoning patterns through reinforcement learning alone.[^7]

For AETHRA, the useful target is not open-ended chain-of-thought imitation. The target
is short, verifiable task behavior:

- identify missing information;
- form a semantic plan of two to six steps;
- select the next capability;
- bind arguments to observed evidence;
- interpret a tool result;
- decide whether to continue, clarify or hand off.

Teacher output must be converted into typed plans and calls, checked by deterministic
validators and reviewed before training. Provider terms must also permit the intended
training use. For example, OpenAI's current Services Agreement restricts using output
to develop competing AI models outside stated exceptions.[^8] AETHRA should prefer
human-authored data, open datasets and outputs from open-weight teachers whose licenses
permit derivative training. A consumer chat subscription is not a training-data
license.

### Post-train a better compact base model

A 0.6B-1.2B foundation has roughly one to two orders of magnitude more parameters
than Needle. Low-rank supervised fine-tuning can specialize one of these models for
AETHRA plans and observations. Preference optimization or reinforcement learning can
follow only after supervised behavior is stable and the reward can be calculated from
tool outcomes.

Training happens on a workstation or rented accelerator. The Android app only loads
the exported inference artifact. Python remains evaluation and training tooling and is
not shipped in the production app.

### Train a new architecture from scratch

This would be required to literally add or redesign reasoning layers while preserving
a coherent pretrained model. It requires a tokenizer, large pretraining corpus,
distributed training, post-training, evaluation and custom mobile kernels. It is far
beyond the efficient path for this project. Native ternary models such as BitNet also
derive much of their benefit from being trained with ternary weights from the start;
ordinary post-training compression does not reproduce the same model.[^9]

## What 1-bit models actually offer

“1-bit LLM” commonly refers to BitNet b1.58. Its linear weights take three values:
`-1`, `0` and `+1`. Encoding three states requires about 1.58 bits of information,
while activations remain 8-bit in Microsoft's released model. The model was trained
natively with that scheme rather than compressed from a conventional checkpoint.[^9]

For a 2.4B-parameter model, 1.58 bits per weight is approximately 474 MB of raw linear
weights before embeddings, scales, runtime buffers and KV state. This is substantially
smaller than BF16, but it is not a 14 MB model and its total process memory is higher
than the raw-weight calculation.

Microsoft reports 0.4 GB of non-embedding memory for BitNet b1.58 2B4T, along with
quality competitive with other one-to-two-billion-parameter models on its selected
benchmarks.[^9] Its official `bitnet.cpp` project reports ARM CPU speedups and energy
reductions versus its comparison path, and lists ARM support for the 2.4B model.[^10]
These results show that native ternary inference is technically important.

They do not establish BitNet as AETHRA's best model:

- the released model has a 4096-token context and limited non-English support;
- Microsoft labels it an R&D model and recommends further testing before real-world
  commercial use;
- efficient execution requires specialized `bitnet.cpp` kernels;
- current upstream ARM issue reports include incorrect output paths and activation
  mismatches, making commit pinning and golden-output tests essential;[^11]
- there is no published AETHRA-style benchmark for ambiguous Android tool use,
  observation-driven planning or safe abstention.

The correct interpretation is that 1.58-bit training creates room for more parameters
within a memory budget. Reasoning still depends on architecture, data, post-training
and inference-time behavior. Low precision alone does not make a model more capable.

## Candidate model landscape

Memory values marked “estimate” are raw Q4 weight calculations or planning ranges,
not measured Android process memory. APK or model-file size must not be confused with
peak proportional set size (PSS).

| Candidate | Intended role | Published size/evidence | Expected mobile footprint | Research judgment |
| --- | --- | --- | --- | --- |
| Needle 2 | Tool routing and extraction | 45M; vendor reports 14 MB binary and ~28 MB session[^1] | Lowest | Retain as baseline after upgrading and validating the current ABI |
| Falcon-H1-Tiny Tool 90M | Alternative tiny tool caller | 91M; official GGUF variants exist[^12] | Roughly 60-140 MB estimate | Cheap challenger; maturity and AETHRA tool accuracy are unknown |
| FunctionGemma 270M | Fine-tuned Android action router | 270M; Google provides a Mobile Actions fine-tuning and LiteRT-LM path[^13] | Roughly 180-350 MB planning range | Highest-priority Needle challenger because deployment and training match this use case |
| Falcon-H1-Tiny-R 0.6B | Small English reasoner | 600M hybrid Transformer/Mamba; official GGUF available[^14] | Roughly 350-700 MB estimate | Highest-priority low-memory reasoning candidate; needs agent/tool post-training |
| Qwen3.5 0.8B | Text, vision and tool-use experiment | 0.8B class, thinking and multimodal support; vendor TAU2 score is only 11.6[^15] | Roughly 500-900 MB estimate | Useful for screenshot understanding; too weak to assume autonomous reliability |
| LFM2.5 1.2B Thinking | Local quality ceiling | Liquid reports under 900 MB on a phone and improved tool benchmark results[^16] | Up to about 900 MB published claim | Strongest on-demand reasoner candidate; too heavy to keep resident continuously |
| BitNet b1.58 2B4T | Native ternary reasoning research | ~2B class, W1.58A8, 4096 context[^9] | At least hundreds of MB plus embeddings/cache | Research branch only until Android correctness and AETHRA task quality are measured |
| Gemini Nano through ML Kit | OS-shared local generation | No model bundled by app; supported devices only[^17] | Managed by AICore | Valuable optional provider, but the current supported-device list does not include OnePlus Nord 5 and background inference is blocked |

FunctionGemma is explicitly intended to be fine-tuned for a defined function surface,
not used as a general dialogue model.[^13] It therefore competes with Needle rather
than replacing the reasoning tier. Google already publishes a Mobile Actions example
that turns commands into flashlight, contact, email, map, Wi-Fi and calendar calls,
and exports the result to LiteRT-LM.[^18]

Falcon-H1-Tiny-R-0.6B is trained specifically for reasoning, but its model card lists
English as its language and does not claim robust Android tool use.[^14] Its value is
as a base for a short-plan and observation-reasoning adapter, not as an immediate
autonomous agent.

Qwen3.5-0.8B is unusually interesting because it combines text and image input with
thinking and tool-use formats. Its own card describes this size as suitable for
prototyping and task-specific fine-tuning. The published TAU2 result is a reminder
that multimodality and a reasoning trace do not guarantee successful agent behavior.[^15]

Liquid's 1.2B Thinking model is the clearest current demonstration that a useful local
reasoner can fit near the user's original 1 GB concern. Liquid reports that the
thinking variant fits within 900 MB on a phone, while its instruct variant used 719 MB
in an S25 Ultra CPU test.[^16][^19] This is compatible with load-on-demand operation on
an 8-12 GB phone, but it still needs physical testing for thermal load, foreground app
pressure and cold-start latency.

## Broad-device compatibility strategy

Compatibility must mean that the AETHRA application, policy engine and deterministic
tools work across the supported Android range. It cannot mean that every phone runs
the same large model at the same speed. The app should discover capabilities and
degrade honestly: a low-memory phone may run only the tiny router, while a newer phone
can add an on-demand reasoner or hardware acceleration.

The proposed initial support floor is Android 12 and ARM64-v8a. That covers the current
product milestone while avoiding a second native ABI and old Android lifecycle paths
before the agent is reliable. Devices outside that floor should be treated as a later
port, not silently offered an untested APK.

At first launch, after an app or model update, and when an optional pack is installed,
the runtime should create a local capability profile from:

- Android API level, ABI and useful CPU instruction features;
- memory class, currently available memory and storage;
- Vulkan version and the availability of supported GPU/NPU delegates;
- AICore/ML Kit feature availability rather than the marketing name of the phone;
- a short backend correctness test and a bounded latency/PSS benchmark;
- thermal status, battery state and recent out-of-memory failures.

The profile selects an approved provider configuration. It must never select a backend
only because a device model appears on a static list. A driver update or OEM firmware
can change accelerator behavior without changing the device name.

| Runtime option | Device reach | Best experimental use | Limitation |
| --- | --- | --- | --- |
| `llama.cpp` with GGUF and ARM CPU kernels | Broadest Android ARM64 baseline | Compare 0.6B-1.2B reasoners with one portable artifact format | CPU latency and battery cost; no universal vendor NPU path |
| LiteRT-LM | Android-native CPU/GPU/NPU where the chosen model/backend is supported | FunctionGemma and supported Gemma-family packs | Model conversion and delegate support vary by device |
| ExecuTorch | XNNPACK CPU baseline plus Vulkan, Qualcomm, MediaTek, Arm and Samsung backend experiments | Determine whether one export pipeline can cover several SoC families | Backend/operator coverage and packaging must be verified per model[^23] |
| MLC LLM | Vulkan-capable Android phones | Portable GPU acceleration experiment | Requires model compilation and driver-specific testing[^24] |
| ML Kit GenAI / Gemini Nano | Phones for which AICore exposes the requested feature | Zero model download by AETHRA and OS-managed local inference | Supported-device subset, quotas, and foreground-only inference[^17] |
| `bitnet.cpp` | ARM64 devices that pass golden-output tests | Native 1.58-bit quality-per-memory research | Separate runtime; current ARM correctness must be proven |

Android's Neural Networks API should not be the common abstraction. Android now marks
NNAPI deprecated in Android 15 and recommends alternatives such as LiteRT GPU runtime
and AICore.[^22] AETHRA's provider contract should sit above these runtimes so one
deprecated Android API cannot force an orchestrator rewrite.

ExecuTorch is useful evidence that accelerator coverage is possible, but it also shows
why AETHRA needs a portable CPU baseline: its Android documentation lists XNNPACK,
Vulkan, Qualcomm, MediaTek, Arm and Samsung paths as distinct backends.[^23] LiteRT's
NPU documentation similarly exposes vendor-specific delegates. "Runs on Android" does
not by itself mean one binary will use every NPU.[^25]

### Downloadable model packs

The base APK should contain the Rust orchestrator, Kotlin platform bridge, policy/UI,
and at most the tiny routing runtime required for an offline command baseline. Larger
models should be signed, downloadable packs rather than inflating every installation.

| Pack | Intended devices | Contents | Proposed behavior |
| --- | --- | --- | --- |
| `router-tiny` | All supported ARM64 phones that pass its self-test | Needle or winning approximately 50-100M router | Narrow local routing and argument extraction |
| `router-quality` | Phones with enough measured headroom | Fine-tuned FunctionGemma 270M | Higher-accuracy local routing; replaces rather than duplicates `router-tiny` in memory |
| `reasoner-lite` | Typically 6 GB+ phones, subject to benchmark | Winning 0.6B Q4-class model | Short plans, summaries and drafts, loaded per session |
| `reasoner-plus` | Typically 8 GB+ phones, subject to benchmark | Winning 0.8B-1.2B quantized model | Higher local quality, unloaded on idle or memory pressure |
| `vision` | Devices that pass image-input memory tests | A compatible multimodal model or adapter | Screenshot understanding only when accessibility semantics are insufficient |
| `ternary-lab` | Opt-in research devices | BitNet model and `bitnet.cpp` runtime | Separate experiment; never the automatic production fallback |

RAM figures in this table are scheduling hypotheses, not hardcoded compatibility
rules. Android memory pressure, model context and backend allocation matter more than
advertised device RAM. Each pack manifest should record the model and adapter IDs,
artifact hash, license, ABI, runtime/backend version, quantization, expected storage,
context limit and a rollback-compatible schema version. A failed checksum, license
acceptance, correctness test or memory benchmark leaves the previous known-good pack
active.

### Privacy modes

Local inference protects sensitive data only if the surrounding pipeline also keeps
contacts, notifications, screenshots, tool results and traces local. AETHRA should
offer explicit modes with no silent fallback:

| Mode | Where reasoning runs | What leaves the phone | Failure behavior |
| --- | --- | --- | --- |
| Strict device-only | Router and optional reasoner on the phone | Nothing | State that the request exceeds local capability |
| Private LAN | Phone plus a user-controlled desktop/home server | Bounded encrypted task packet on the local network | Ask before changing to public cloud |
| Minimized cloud | Local router/redactor plus configured cloud model | User-visible, policy-approved task packet | Stop if required sensitive fields cannot be withheld |
| Cloud enabled | Local routing plus configured cloud reasoner | Bounded context allowed by task policy | Still require deterministic approval for external writes |

Private LAN inference is the strongest compatibility option for older phones when the
user wants better reasoning without sending content to a public model provider. It is
not identical to on-device privacy because data leaves the handset, so the UI and audit
log must describe it accurately. Confidential cloud/TEE offerings can reduce provider
visibility, but data still crosses the device boundary and implementations are
provider-specific.

Local redaction is useful for some cloud tasks: AETHRA can replace a contact, phone
number or message body with typed placeholders and restore them only when a validated
local tool executes. It is not a universal solution because many requests require the
model to understand the sensitive text itself. In those cases the user chooses local,
LAN or explicit cloud processing.

### Coexistence with games and other applications

An Android phone cannot be scheduled like a dedicated inference server. Android keeps
background applications in a cache and kills cached processes when the foreground
workload needs memory; applications that retain more memory also make the rest of the
multitasking experience worse.[^26] AETHRA must treat process death and model eviction
as normal operation.

The reasoner therefore must not remain resident merely to reduce its next cold start.
Use explicit operating states:

| State | Resident work | Transition rule |
| --- | --- | --- |
| Idle | Persisted task state and optional tiny trigger only | No local LLM allocation |
| Listening | Voice activity/wake-word component | Start only through Android-supported microphone lifecycle |
| Routing | Tiny router for one bounded inference | Unload it too if the device profile says its idle cost is excessive |
| Reasoning | One selected reasoner and short context | User-started session with visible progress and a time/token budget |
| Acting | Accessibility/tool execution plus persisted state | Keep the reasoner only when the next observation requires it |
| Pressured | Deterministic tools and persisted state only | Cancel inference, release model memory and defer or ask for an allowed fallback |

When the user starts an automation that operates over another foreground application,
AETHRA may use a visible, time-bounded foreground service for that active session.
Foreground-service status makes the work user-visible and raises process importance;
it does not provide unlimited memory or permission to run indefinitely. Android states
that long-running services become increasingly likely to be killed, and background
execution is subject to additional restrictions.[^27]

The runtime should respond to `onTrimMemory`, `ActivityManager.MemoryInfo.lowMemory`,
available-memory thresholds, model-allocation failures and process-exit history. It
should checkpoint the semantic plan before each external action, release model and KV
cache allocations promptly, and reconstruct the inference worker from persisted state
after safe process death. The model worker should be isolated from the durable Rust
orchestrator state so releasing or losing the worker cannot erase approvals or leave a
send action in an assumed-success state.

Thermal throttling is a separate limit from RAM. Android documents that devices can
sustain high performance only for a limited period and that thermal behavior varies
with device design, ambient conditions and recent use.[^28] Before and during local
generation, AETHRA should sample thermal headroom at the documented rate. At severe
pressure it should reduce the output budget or stop inference; it should never compete
with a foreground game by repeatedly unloading and reloading the model.

This creates three honest service levels:

- lightweight commands can remain local on most supported devices;
- complex local reasoning is available only while current resource headroom permits;
- under a game, camera session or other heavy workload, AETHRA may defer, use the
  user-configured private LAN provider, request an explicitly permitted cloud handoff,
  or state that the command cannot run now.

The voice design follows the same split. A wake detector may be continuous if its
measured DSP/CPU and battery cost is acceptable. Speech recognition and the LLM are
activated for a request; a continuously generating LLM is not part of the idle
assistant.

## Recommended target runtime

The model pipeline should have two local slots, not one model expected to do every
job:

| Slot | Default | Purpose | Lifecycle |
| --- | --- | --- | --- |
| `LocalRouter` | Needle 2 or winning FunctionGemma tune | Select one tool, extract arguments, detect handoff | Small enough to remain warm if device measurements allow |
| `LocalReasoner` | Winning 0.6B-1.2B candidate | Short planning, summarization, drafting, observation interpretation | Downloadable; load for a session; unload on idle or memory pressure |
| `CloudReasoner` | User-configured provider | Difficult planning, broad knowledge and high-quality language | Called only with explicit cloud policy and bounded context |

Rust chooses the slot from observable task properties. A direct device command goes
to `LocalRouter`. Requests needing prose, summarization or a plan go to
`LocalReasoner`. Unsupported language, repeated local failure, long context or a
complex cross-domain task may go to `CloudReasoner`. A model can request escalation,
but it cannot grant itself network access or a larger data scope.

Each inference receives a typed packet:

```text
objective
current semantic step
confirmed facts
bounded recent observations
3-8 selected capability schemas
required output schema
remaining step, token, latency and egress budget
```

The local reasoner should produce short artifacts: a plan, a tool call, a question or
a concise draft. Long hidden reasoning consumes battery and often creates additional
failure opportunities. Verification should happen through tools and typed state, not
through repeated self-reflection.

The runtime must track model artifact hash, quantization, backend, context size,
sampling parameters and adapter revision with every result. Without provenance, an
improvement cannot be reproduced.

## Training program

### Data contract

Create an AETHRA trajectory format containing:

- the user request;
- normalized device facts;
- retrieved tool schemas;
- expected semantic plan;
- expected next action and exact arguments;
- simulated or recorded tool observations;
- expected clarification, abstention or handoff;
- safety labels and approval boundary;
- final response criteria.

Raw personal notifications, contacts and messages must not become training fixtures.
Generate synthetic identities and screen observations with the same schemas. Real user
failures can be converted into redacted structural examples only after explicit
opt-in.

### Curriculum

Train and evaluate in increasing difficulty:

1. Single obvious tool.
2. Similar competing tools.
3. Exact argument extraction.
4. Missing and ambiguous arguments.
5. Off-topic input and no-call behavior.
6. One observation followed by a next action.
7. Stale or contradictory observations.
8. Two-to-six-step semantic plans.
9. Tool failure and plan repair.
10. Prompt injection inside tool output.

Tool routing and language quality remain separate datasets and metrics. A model that
chooses the right SMS tool but produces poor wording has passed routing and failed
drafting.

### Training methods

Use supervised fine-tuning first. Train the exact structured outputs AETHRA accepts,
with substantial negative and ambiguity examples. LoRA or QLoRA is sufficient for the
first 0.6B-1.2B experiments. Merge and quantize only after float validation passes.

Preference optimization can later compare valid alternative plans. Reinforcement
learning is justified only where the reward comes from deterministic task completion,
argument correctness and safe stopping. A language-model judge should not be the sole
reward for external actions.

For Needle, follow upstream's quantization-aware LoRA path but test the float adapter,
2-bit export and 4-bit export against identical golden cases. For FunctionGemma, use
Google's Mobile Actions pipeline and export `.litertlm`. For the reasoners, begin with
GGUF through `llama.cpp` for broad CPU compatibility; consider LiteRT-LM only after the
model is supported and its Android backend is measurably better.

## Inference runtime choices

`llama.cpp` is the safest first reasoner runtime because it has an Android binding,
NDK cross-compilation guidance and broad GGUF support. Its Android documentation warns
that context size can cause memory spikes and suggests beginning around 4096 tokens.[^20]
AETHRA should therefore use a short context even if a model advertises hundreds of
thousands of tokens.

LiteRT-LM is the strongest Android-native challenger. Google describes stable Kotlin
and C++ APIs, CPU/GPU/NPU backends, multimodality and tool use. Its Kotlin guide warns
that model initialization can take up to ten seconds and requires explicit engine
closure to release resources.[^21] It is especially attractive for FunctionGemma and
supported Gemma-family experiments.

Gemini Nano through ML Kit would avoid bundling a model and server cost on supported
phones. The current ML Kit documentation lists the supported devices, applies per-app
battery quotas and blocks inference whenever the app is not the top foreground app,
even from a foreground service.[^17] It therefore cannot be AETHRA's universal
background brain. It can be an opportunistic provider on compatible devices.

Qualcomm NPU work should begin only after CPU baselines pass. The Snapdragon 8s Gen 3
contains suitable AI hardware, but production access depends on supported operators,
model conversion, vendor libraries and device firmware. An NPU demo that changes the
model or tokenizer is not comparable to the CPU baseline.

## Evaluation and decision gates

The existing 60-scenario companion suite is a useful behavioral skeleton but all
model-quality results are currently `not_run`. Expand it into at least 300 development
cases and a separately authored 100-case holdout. Keep real-device performance cases
separate from semantic-quality cases.

Measure:

- exact tool selection;
- exact and schema-valid arguments;
- ambiguity detection and clarification recall;
- correct abstention on unsupported requests;
- semantic plan validity;
- completion after successive observations;
- drafting and summarization quality;
- unnecessary cloud escalation rate;
- total supported-task completion;
- unsafe proposal rate before PolicyEngine;
- cost and tokens for cloud-assisted cases.

On the phone, record:

- artifact and runtime hashes;
- cold load time and warm first-token latency;
- prompt and decode tokens per second;
- peak PSS and idle PSS;
- battery consumed per fixed workload;
- CPU/GPU utilization and thermal state;
- foreground-app eviction, ANRs and crashes;
- unload time and successful reload after memory pressure.

Proposed go/no-go gates for the first research milestone are:

| Gate | Target |
| --- | --- |
| Router exact tool on supported direct commands | At least 95% on holdout |
| Exact required arguments | At least 90% on holdout |
| Ambiguous recipient clarification | At least 95% |
| External write without PolicyEngine approval | Zero |
| Incorrect automatic retry after uncertain write | Zero |
| Warm local-reasoner first token | At most 2.5 seconds on target phone |
| Cold load | At most 8 seconds with visible progress |
| 0.6B-tier peak PSS | At most 750 MB |
| Idle assistant after unload | At most 100 MB attributable process PSS |
| Value of local reasoner | At least 15 percentage-point task-success gain over router-only, or at least 30% fewer cloud calls at no material quality loss |

These are product gates, not claims about current models. If no candidate passes, the
architecture should retain Needle for narrow routing and use cloud reasoning. It
should not conceal the failure by lowering the benchmark.

## Experimental sequence

### Experiment 0: compatibility profiler and device matrix

- Implement a read-only Android capability probe and provider self-test behind a
  typed Rust contract.
- Test at least 4 GB, 6 GB, 8 GB and 12 GB RAM classes across representative
  Qualcomm, MediaTek, Exynos and Tensor devices where physical hardware is available.
- Record CPU fallback on every device; add GPU/NPU results only when correctness
  matches the CPU golden outputs.
- Measure cold load, first token, tokens/second, peak and post-unload PSS, battery use,
  thermal throttling, app switching and background process survival.
- Repeat the run with a memory-heavy foreground workload, with the screen off where
  Android permits the operation, and after synthetic trim-memory events.
- Verify that model eviction preserves the semantic plan, cancels uncertain actions,
  and does not cause an automatic external-write retry after process restart.
- Measure collateral impact: other-app relaunches, AETHRA PSS after unload, cold and
  warm task latency, thermal headroom, battery drain and throttling over 20 minutes.
- Verify that the base assistant still opens, explains limitations and performs its
  deterministic tools when no optional model pack qualifies.

**Decision:** publish a model pack only for capability profiles on which its artifact,
backend and memory gates pass. Device marketing names remain diagnostic metadata, not
the policy key.

### Experiment 1: repair the router baseline

- Update the Needle integration in an isolated branch to a pinned current upstream
  version.
- Capture the full structured response, including confidence where available.
- Run 300 router cases against the existing Needle archive before training.
- Test strict grammar, empty-call abstention, competing tools and successive results.

**Decision:** continue Needle training only if the current upstream runtime removes the
invalid-response pattern and reaches a credible baseline.

### Experiment 2: router bake-off

- Fine-tune Needle on the same AETHRA routing corpus.
- Fine-tune FunctionGemma 270M using the same semantic examples and export through
  LiteRT-LM.
- Add Falcon-H1-Tiny Tool 90M as an inexpensive third baseline if its GGUF parser and
  license pass review.
- Compare quality, PSS, cold load, latency and battery across the Experiment 0 device
  matrix, using the OnePlus phone as one real-device baseline.

**Decision:** ship one router. Multiple tiny routers in the APK provide no user value.

### Experiment 3: local reasoning bake-off

- Quantize and test Falcon-H1-Tiny-R-0.6B.
- Test Qwen3.5-0.8B in text-only mode; evaluate screenshot input separately.
- Test LFM2.5-1.2B-Thinking as the quality ceiling.
- Give each model the identical typed AETHRA packet and maximum output budget.

**Decision:** select the smallest candidate that meets the task-success and cloud-call
reduction gates. Load it on demand rather than keeping it resident.

### Experiment 4: 1.58-bit branch

- Cross-compile a pinned `bitnet.cpp` commit for Android ARM64.
- Run published golden prompts before AETHRA evaluation to detect kernel corruption.
- Benchmark BitNet b1.58 2B4T and Falcon3 1B 1.58-bit if both produce stable output.
- Compare against the winning Q4 0.6B-1.2B model at equal context and output length.

**Decision:** adopt a ternary runtime only if it improves the measured quality-per-PSS
or quality-per-joule frontier and passes all golden outputs. A smaller model file alone
is insufficient.

### Experiment 5: AETHRA-specific post-training

- Train the winning local reasoner on verified short plans and observations.
- Add adversarial and failure-recovery examples.
- Evaluate float, quantized and mobile artifacts independently.
- Conduct a physical-device 20-minute thermal and lifecycle run.

**Decision:** make the tuned reasoner downloadable only after it beats the untouched
base on the hidden holdout without increasing unsafe proposals.

## Product implication

If this research succeeds, AETHRA's defensible asset is not a generic chat model. It
is an evaluated, replaceable on-device cognition stack trained on real Android task
contracts:

- a tiny local router;
- a compact, task-trained local reasoner;
- deterministic policy and execution;
- a private task dataset and reproducible device benchmark;
- cloud escalation instead of cloud dependence.

That stack could make the phone agent cheaper, more private and more responsive for
its supported tasks while retaining frontier reasoning when required. If the local
reasoner does not reduce cloud use or improve task completion, AETHRA should remain a
cloud-powered execution layer rather than claiming a model advantage.

## Sources

[^1]: Cactus Compute, [Needle 2 technical/API summary](https://github.com/cactus-compute/needle/blob/main/llms.txt), accessed September 2026.
[^2]: OnePlus, [OnePlus Nord 5 product specifications](https://www.oneplus.in/nord-5), accessed September 2026.
[^3]: Qualcomm, [Snapdragon 8s Gen 3 Mobile Platform product brief](https://docs.qualcomm.com/bundle/publicresource/87-73942-1_REV_B_Snapdragon_8s_Gen_3_Mobile_Platform_Product_Brief.pdf), accessed September 2026.
[^4]: Cactus Compute, [Needle fine-tuning guide](https://github.com/cactus-compute/needle/blob/main/doc/finetuning.md), accessed September 2026.
[^5]: Cactus Compute, [Needle API confidence behavior](https://github.com/cactus-compute/needle/blob/main/doc/apis.md), accessed September 2026.
[^6]: Cactus Compute issue tracker, [Needle quantized export behavior report #91](https://github.com/cactus-compute/needle/issues/91), August 2026. This is an unresolved user report, not a confirmed general defect.
[^7]: DeepSeek-AI, [DeepSeek-R1: reasoning distillation results](https://github.com/deepseek-ai/DeepSeek-R1), 2025.
[^8]: OpenAI, [OpenAI Services Agreement](https://openai.com/policies/services-agreement/), effective January 2026.
[^9]: Microsoft Research, [BitNet b1.58 2B4T model card and technical results](https://huggingface.co/microsoft/bitnet-b1.58-2B-4T), 2025.
[^10]: Microsoft, [`bitnet.cpp` official inference framework](https://github.com/microsoft/BitNet), accessed September 2026.
[^11]: Microsoft BitNet issue tracker, [ARM64 output report #600](https://github.com/microsoft/BitNet/issues/600) and [activation mismatch report #602](https://github.com/microsoft/BitNet/issues/602), 2026. These are issue reports and require reproduction.
[^12]: Technology Innovation Institute, [Falcon-H1-Tiny model collection](https://huggingface.co/collections/tiiuae/falcon-h1-tiny), 2026.
[^13]: Google DeepMind, [FunctionGemma model overview](https://ai.google.dev/gemma/docs/functiongemma), December 2025.
[^14]: Technology Innovation Institute, [Falcon-H1-Tiny-R-0.6B model card](https://huggingface.co/tiiuae/Falcon-H1-Tiny-R-0.6B), 2026.
[^15]: Qwen Team, [Qwen3.5-0.8B model card](https://huggingface.co/Qwen/Qwen3.5-0.8B), 2026.
[^16]: Liquid AI, [LFM2.5-1.2B-Thinking: On-Device Reasoning Under 1GB](https://www.liquid.ai/blog/lfm2-5-1-2b-thinking-on-device-reasoning-under-1gb), January 2026.
[^17]: Google ML Kit, [Overview of the ML Kit GenAI APIs](https://developers.google.com/ml-kit/genai), updated September 2026.
[^18]: Google AI for Developers, [Fine-tune FunctionGemma for Mobile Actions](https://ai.google.dev/gemma/docs/mobile-actions), 2025.
[^19]: Liquid AI, [Introducing LFM2.5](https://www.liquid.ai/blog/introducing-lfm2-5-the-next-generation-of-on-device-ai), January 2026.
[^20]: ggml-org, [`llama.cpp` Android documentation](https://github.com/ggml-org/llama.cpp/blob/master/docs/android.md), accessed September 2026.
[^21]: Google AI Edge, [LiteRT-LM Kotlin getting started](https://github.com/google-ai-edge/LiteRT-LM/blob/main/docs/api/kotlin/getting_started.md), accessed September 2026.
[^22]: Android Developers, [Neural Networks API](https://developer.android.com/ndk/guides/neuralnetworks) and [NNAPI migration guide](https://developer.android.com/ndk/guides/neuralnetworks/migration-guide), accessed September 2026.
[^23]: PyTorch, [ExecuTorch Android backends](https://docs.pytorch.org/executorch/stable/android-backends.html), accessed September 2026.
[^24]: MLC team, [MLC LLM Android deployment](https://llm.mlc.ai/docs/deploy/android), accessed September 2026.
[^25]: Google AI Edge, [LiteRT NPU acceleration](https://ai.google.dev/edge/litert/android/npu), accessed September 2026.
[^26]: Android Developers, [Overview of Android memory management](https://developer.android.com/topic/performance/memory-overview), accessed September 2026.
[^27]: Android Developers, [Service process lifecycle](https://developer.android.com/reference/android/app/Service) and [background tasks overview](https://developer.android.com/develop/background-work/background-tasks), accessed September 2026.
[^28]: Android Developers, [Android Thermal API](https://developer.android.com/games/optimize/adpf/thermal), accessed September 2026.
