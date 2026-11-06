# Embedding models and planned retrieval improvements

Specifications checked against the linked publisher model cards/configurations on
2026-10-23; BGE-base and reranker cards rechecked on 2026-11-02. MiniLM,
BGE-small and BGE-base are selectable embedding providers. BGE-base is the default
as of November 4. The reranker experiment was removed; its results below are
historical. Other rows remain candidates.

## Current models and larger candidates

| Model | Approx. parameters | Layers | Vector dimensions | Input tokens | Approx. raw weights | Language / license | Pintrail status |
|---|---:|---:|---:|---:|---|---|---|
| [all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) | 22.7M | 6 | 384 | 256 configured | 91 MB, FP32 | English; Apache-2.0 | Implemented and evaluated |
| [bge-small-en-v1.5](https://huggingface.co/BAAI/bge-small-en-v1.5) | 33.4M | 12 | 384 | 512 | 133 MB, FP32 | English; MIT | Comparison/fallback choice |
| [bge-base-en-v1.5](https://huggingface.co/BAAI/bge-base-en-v1.5) | 109.5M | 12 | 768 | 512 | 438 MB, FP32 | English; MIT | Default; evaluated November 2 |
| [bge-large-en-v1.5](https://huggingface.co/BAAI/bge-large-en-v1.5) | 335.1M | 24 | 1,024 | 512 | 1.34 GB, FP32 | English; MIT | Larger CPU-cost candidate |
| [bge-m3](https://huggingface.co/BAAI/bge-m3) | about 568–569M | 24 | 1,024 dense | 8,192 | about 2.27 GB, FP32 | Multilingual; MIT | Longer-context alternative, not wired into provider |
| [Qwen3-Embedding-0.6B](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) | 595.8M | 28 | up to 1,024; configurable | 32K advertised | 1.19 GB, BF16 | 100+ languages; Apache-2.0 | More demanding candidate; integration/hardware test required |
| [Qwen3-Embedding-4B](https://huggingface.co/Qwen/Qwen3-Embedding-4B) | 4.02B | 36 | up to 2,560; configurable | 32K advertised | 8.04 GB, BF16 | Multilingual; Apache-2.0 | Future resource-dependent comparison |
| [Qwen3-Embedding-8B](https://huggingface.co/Qwen/Qwen3-Embedding-8B) | 7.57B (8B family name) | 36 | up to 4,096; configurable | 32K advertised | 15.13 GB, BF16 | Multilingual; Apache-2.0 | Future resource-dependent comparison |

Parameters describe model size; vector dimensions describe each saved output;
input tokens describe accepted text length. They are different quantities.
Raw-weight sizes are arithmetic estimates using rounded parameter counts and
four bytes per FP32 or two per BF16 parameter, in decimal MB/GB. They are **not**
download sizes, RAM/VRAM requirements, or server sizing promises. Tokenization,
activations, runtime buffers and batch/context length add memory. Quantized exports
have different sizes and must be evaluated separately.

Parameter counts for the MiniLM/BGE English/Qwen rows come from publisher
Hugging Face model metadata; layers come from their configs. BGE-M3's approximate
parameter size is also documented in the [publisher's BGE model overview](https://bge-model.com/tutorial/1_Embedding/1.2.1.html).
Qwen 4B/8B configs contain 40,960 position entries, but the publisher's embedding
model table advertises 32K sequence length; the table above uses that advertised
retrieval limit rather than assuming a larger supported input.

MiniLM's base architecture has 512 positions, but its sentence-embedding model
card defaults to 256 word pieces; this provider enforces 256 explicitly. Both
small models and BGE-base use FastEmbed 7.1.5 with FP32 ONNX exports, CPU inference and two
ONNX threads by default. BGE questions receive the recommended retrieval
instruction; documents do not. Token limits include special tokens/instructions.

BGE-base, BGE-large and BGE-M3 appear in the installed fastembed-rs 7.1.5 model
registry; our `Model` enum/configuration accepts BGE-small, BGE-base and MiniLM.
The registry's BGE-M3 dense output does not automatically implement its sparse or
multi-vector retrieval modes in Pintrail. Qwen is not integrated into this provider;
its serving/export/runtime path and the actual VM's resources need checking.
Changing the provider model requires re-embedding documents, which the new
model-fingerprint-aware worker supports. No paid embedding API is required;
hosting and compute still consume resources.

## Earlier local results (October 23)

The latest frozen fixture contains 64 supported questions over 58 artifacts plus
four unsupported requests. It includes the original eight development questions,
24 initial additions and 32 harder follow-ups. These are authored from the seed,
not independent collector questions, and no model weights were trained or tuned.

| Full canonical documents | Expected first | Expected top 3 | MRR |
|---|---:|---:|---:|
| BGE-small | 58/64 | 62/64 | 0.9414 |
| MiniLM | 57/64 | 63/64 | 0.9345 |
| Simple word overlap | 47/64 | 55/64 | 0.8086 |

The model counts were reproduced against the actual saved PostgreSQL vectors.
Expected artifact IDs/evidence are used for scoring only and never supplied to
the embedding model. Unsupported queries are excluded from the supported-hit
denominator and their nearest matches are displayed separately.

For 132 paragraph passages, BGE scored 55/64 first and 63/64 top-three; MiniLM
scored 53/64 first and 63/64 top-three. Simple paragraph splitting was therefore
not consistently better than the current full-document pipeline. The earlier
32-question MiniLM result of 32/32 first did not establish a general advantage.

One-run warm-cache observations on the developer's Ryzen 9 6900HS laptop,
Ubuntu/WSL, CPU, debug build, two ONNX threads:

| Full-document run | Embed 58 documents | Question embedding p50 / p95 | Process peak RSS |
|---|---:|---:|---:|
| BGE-small | 4.15 s | 21.0 / 35.9 ms | 267.7 MiB |
| MiniLM | 2.86 s | 11.1 / 19.3 ms | 206.2 MiB |

These are local observations, not production latency/throughput guarantees or
controlled hardware comparisons. Question timings cover embedding only, not
database lookup or the complete API request. BGE-base was tested later, below.
Machine-readable summaries and per-case ranks are in
`evaluation-results/2026-10-23.json`.

The query “I need a little tropical escape between lectures when everything
outdoors is freezing” placed Durfee fourth with BGE and seventh with MiniLM.
Both models also return candidates for absent content. Similarity is not a
probability that the requested artifact exists; no abstention cutoff is configured.

## November 2: metadata and a larger model

The same frozen 64 supported and four unsupported questions were run against
saved PostgreSQL vectors before and after adding tags, published linked topic
names and per-link notes. The questions and expected labels were unchanged.
The seed remains 58 artifacts; this is not independent collector validation.

| Saved-vector pipeline | Expected first | Expected top 3 |
|---|---:|---:|
| BGE-small, original text | 58/64 | 62/64 |
| MiniLM, original text | 57/64 | 63/64 |
| BGE-small, added metadata | 57/64 | 62/64 |
| MiniLM, added metadata | 57/64 | 63/64 |
| BGE-base, added metadata | 62/64 | 64/64 |

Adding metadata alone did **not** improve these aggregate results. It completes
the searchable source and keeps changes synchronized, but should not be described
as a proven accuracy gain. A separate BGE-base control using the original text
in the in-memory full-document evaluator scored 63/64 first and 64/64 top-three.
That supports attributing the main improvement to the larger embedding model,
rather than to the additional tags/topics. It also shows that more context can
occasionally change an already-correct first result.

BGE-base alone now places Durfee first for the previously missed “tropical
escape” question. The remaining first-place failures with added metadata concern
ValleyBike and solar hot water. Unsupported requests still return candidates;
the evaluation does not treat that as evidence of an answer or invent a cutoff.

In the original-text BGE-base control, embedding 58 documents took 11.29 seconds;
question embedding p50/p95 was 50.2/76.7 ms, and process peak RSS was about 730 MiB.
These are one-run debug CPU observations, with development activity on the same
laptop. They are neither HTTP response times nor controlled production benchmarks.

The configuration default changed to BGE-base on November 4. An explicit
`EMBEDDING_MODEL` setting takes priority. On switching from a smaller model, run
the model-aware worker to backfill distinct 768-dimensional vectors before
searching with BGE-base. The lab already has all 58 BGE-base vectors. Different
models can coexist in storage without mixing vector spaces; only one selected
model is loaded by each worker. BGE-base
is the best current candidate on this fixture; an independent collector set and
the actual server's resource budget should decide the deployment choice.

### Historical reranker results (implementation removed November 4)

Both runs retrieved 20 current published candidates from the expanded-text
vectors, then scored complete question/document pairs with BGE-reranker-base.
Every supported question's expected artifact was already in those 20 candidates.

| Expanded-text pipeline | Expected first | Expected top 3 | Added reranking/check time p50 / p95 |
|---|---:|---:|---:|
| BGE-small alone | 57/64 | 62/64 | Not applied |
| BGE-small + reranker | 57/64 | 63/64 | 5,070 / 5,807 ms |
| BGE-base alone | 62/64 | 64/64 | Not applied |
| BGE-base + reranker | 57/64 | 62/64 | 4,917 / 5,673 ms |

The reranker promoted Durfee to first for BGE-small's missed tropical-escape
question, but introduced other errors. For example, it preferred Surplus Property
Barn over New2U for buying secondhand clothes and furniture. BGE-base already
found Durfee first, and adding the reranker made its aggregate results worse.
The implementation, command option and score field have been removed. BGE-base
**without reranking** is the current path. A larger or second model does not
guarantee better results.

The measured times cover reranking 20 candidates plus database reads and final
publication/hash checks, excluding initial question embedding and vector lookup.
They include some overlap with other development checks on the laptop. These
figures are observations, not a controlled server benchmark. No input exceeded
the enforced 512-token pair limit in these runs. Scores remain raw logits; no
score cutoff or answer confidence is inferred from them.

Machine-readable summaries, actual model snapshot revisions, unchanged fixture
hashes and every question's outcomes are in
[`evaluation-results/2026-11-02.json`](evaluation-results/2026-11-02.json).
The original-text BGE-base control is recorded separately to distinguish the
effect of changing the model from changing its input. Fixture relevance labels
are authored and sometimes select one of several plausible matches; they still
need independent human validation before claiming general search accuracy.

## Improvement plan

1. Tags, linked published topic names and per-link notes now enter canonical text
   with refresh triggers (#66/#72). Decide topic-result behavior (#67).
2. Keyword and hybrid API modes are implemented (#28); compare them below and
   validate with independent collector questions before changing defaults.
3. BGE-base was compared against the same frozen questions above. Test BGE-large,
   BGE-M3 or Qwen0.6B if resources justify them. Compare query latency, indexing
   time, peak memory and failure cases alongside first/top-three accuracy. Keep
   an independent set of collector questions for validation before choosing.
4. Define no-close-match behavior using more negative examples. Do not tune a
   universal similarity threshold to four unsupported prompts.
5. Add production chunking for long descriptions/documents; preserve artifact,
   attachment and page provenance and group passage hits back into artifacts.

The removed reranker was [BAAI/bge-reranker-base](https://huggingface.co/BAAI/bge-reranker-base):
about 278M parameters, 12 layers, English/Chinese, MIT license, roughly 1.11 GB of
FP32 raw weights. It outputs a relevance score, not a reusable embedding vector.
The historical experiment enforced a 512-token combined question/document pair
cap without silent truncation. These specifications describe that removed
experiment, not a currently available provider.

## Future PDFs, captions, images and answers

These text embedders do not read image pixels or PDF containers. A later PDF
pipeline extracts text (OCR for scans), splits passages and keeps page references.
Human captions can be embedded directly; a separate vision component could produce
reviewed image descriptions. The embedding model searches that resulting text.
Direct multimodal retrieval would be a different extension. An answer provider
would later read permitted retrieved sources and generate cited text. These
extensions are not implemented here and are not prerequisites for semantic search.

## November 5: initial keyword/hybrid modes (historical)

The API now supports `mode=keyword|semantic|hybrid`. Keyword retrieval uses
PostgreSQL full-text matching and explicit exact-name priority; hybrid combines
top-50 branch rankings with fixed equal-weight reciprocal-rank fusion (offset
60), with the same exact-name priority. It adds no model and no training.

| Mode | Existing 64: first / top 3 | Added lexical 12: first / top 3 |
|---|---|---|
| Semantic | 62 / 64 | 9 / 10 |
| Keyword | 5 / 5 | 12 / 12 |
| Hybrid | 60 / 63 | 12 / 12 |

Hybrid helps specific word/phrase lookups but regresses some conversational
questions; semantic remains the default. The added cases are authored, not
independent collector questions. Labels and fusion constants were frozen before
measurement. Full results are in
[`2026-11-05-hybrid.json`](evaluation-results/2026-11-05-hybrid.json); the
[API guide](../../services/pintrail-api/SEARCH.md) explains the trade-offs and
reproduction command `postgres_search evaluate-modes`.

### Refined hybrid ordering

The current `semantic-first-v1` policy retains semantic order for ordinary
questions. Keyword support can promote explicit phrase/exclusion searches,
short selective lookups and empty-vector fallbacks; exact names always retain
priority. A short lookup has at most three words, does not begin with a common
question word, and matches one to three approved artifacts. These are provisional
English heuristics, informed by development failures rather than independently
validated query-intent understanding.

| Retrieval | Existing 64: first / top 3 | Lexical 12: first / top 3 | New diagnostic 5: first / top 3 |
|---|---|---|---|
| Semantic | 62 / 64 | 9 / 10 | 4 / 4 |
| Initial hybrid | 60 / 63 | 12 / 12 | Not measured |
| Refined hybrid | 62 / 64 | 12 / 12 | 4 / 4 |

Both known regressions were fixed without changing original questions, expected
labels, corpus content or model weights. The additional diagnostic paraphrases
are authored, not independent student data. The long question about
"electrochromic technology" still fails in semantic and hybrid modes, despite
success on the single-term lookup. Semantic remains the default.

The original report is preserved. Revised results, model identity and fixture
hashes are in
[`2026-11-05-hybrid-refined.json`](evaluation-results/2026-11-05-hybrid-refined.json).
