# Embedding models and planned retrieval improvements

Specifications checked against the linked publisher model cards/configurations on
2026-10-23. Only MiniLM and BGE-small are implemented as selectable providers in
this change. Other rows are candidates, not measured improvements on Pintrail.

## Current models and larger candidates

| Model | Approx. parameters | Layers | Vector dimensions | Input tokens | Approx. raw weights | Language / license | Pintrail status |
|---|---:|---:|---:|---:|---|---|---|
| [all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) | 22.7M | 6 | 384 | 256 configured | 91 MB, FP32 | English; Apache-2.0 | Implemented and evaluated |
| [bge-small-en-v1.5](https://huggingface.co/BAAI/bge-small-en-v1.5) | 33.4M | 12 | 384 | 512 | 133 MB, FP32 | English; MIT | Implemented; provisional configuration default |
| [bge-base-en-v1.5](https://huggingface.co/BAAI/bge-base-en-v1.5) | 109.5M | 12 | 768 | 512 | 438 MB, FP32 | English; MIT | First larger candidate to evaluate |
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
current models use FastEmbed 7.1.5 with FP32 ONNX exports, CPU inference and two
ONNX threads by default. BGE questions receive the recommended retrieval
instruction; documents do not. Token limits include special tokens/instructions.

BGE-base, BGE-large and BGE-M3 appear in the installed fastembed-rs 7.1.5 model
registry, but our `Model` enum/configuration still accepts only BGE-small and MiniLM.
The registry's BGE-M3 dense output does not automatically implement its sparse or
multi-vector retrieval modes in Pintrail. Qwen is not integrated into this provider;
its serving/export/runtime path and the actual VM's resources need checking.
Changing the provider model requires re-embedding documents, which the new
model-fingerprint-aware worker supports. No paid embedding API is required;
hosting and compute still consume resources.

## Measured local results

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
database lookup or the complete API request. Larger candidates have not been
benchmarked locally. Machine-readable summaries and per-case ranks are in
`evaluation-results/2026-10-23.json`.

The query “I need a little tropical escape between lectures when everything
outdoors is freezing” placed Durfee fourth with BGE and seventh with MiniLM.
Both models also return candidates for absent content. Similarity is not a
probability that the requested artifact exists; no abstention cutoff is configured.

## Improvement plan

1. Complete tags, linked topic names and per-link notes in canonical text and
   refresh triggers (#66/#72); decide topic-result behavior (#67).
2. Implement lexical plus vector candidate retrieval and hybrid ranking (#28).
3. Compare BGE-base first against the same frozen questions, then BGE-large,
   BGE-M3 or Qwen0.6B if resources justify them. Compare query latency, indexing
   time, peak memory and failure cases alongside first/top-three accuracy. Keep
   an independent set of collector questions for validation before choosing.
4. Evaluate retrieve-and-rerank: retrieve more candidates (for example 20), then
   score each question/document pair using a second model. Durfee is already in
   BGE's top 20 in the failed example; reranking could help, but is not tested.
   See [Sentence Transformers' retrieve/rerank guide](https://www.sbert.net/examples/sentence_transformer/applications/retrieve_rerank/README.html).
5. Define no-close-match behavior using more negative examples. Do not tune a
   universal similarity threshold to four unsupported prompts.
6. Add production chunking for long descriptions/documents; preserve artifact,
   attachment and page provenance and group passage hits back into artifacts.

One reranker candidate is [BAAI/bge-reranker-base](https://huggingface.co/BAAI/bge-reranker-base):
about 278M parameters, 12 layers, English/Chinese, MIT license, roughly 1.11 GB of
FP32 raw weights. It outputs a relevance score, not a reusable embedding vector.
A 512-token **combined question/document pair cap** is a suitable initial test
configuration; it needs separate accuracy/latency and truncation testing. It is
not implemented in this change. A reranker only reorders retrieved candidates;
it cannot recover an artifact absent from the candidate set.

## Future PDFs, captions, images and answers

These text embedders do not read image pixels or PDF containers. A later PDF
pipeline extracts text (OCR for scans), splits passages and keeps page references.
Human captions can be embedded directly; a separate vision component could produce
reviewed image descriptions. The embedding model searches that resulting text.
Direct multimodal retrieval would be a different extension. An answer provider
would later read permitted retrieved sources and generate cited text. These
extensions are not implemented here and are not prerequisites for semantic search.
