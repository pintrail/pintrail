# Local embedding component

This is the embedding portion of issue #26: a shared Rust trait, a real CPU
FastEmbed implementation, and a deterministic substitute for application tests.
An optional PostgreSQL laboratory saves and searches vectors. An opt-in worker
now generates embeddings automatically as prepared text changes. The HTTP
`/search` endpoint and production rollout are still pending.
No model training or paid API key is required.

See [model specifications and improvement plan](MODEL_COMPARISON.md) for the
current models, larger candidates, latest results and measured limitations.

## How it fits

`EmbeddingProvider` defines text-in/vectors-out. `LocalProvider` loads one model
and reuses it across calls. `InputKind` distinguishes documents from questions:
BGE's recommended retrieval instruction is applied to questions only. The
provider rejects empty inputs, oversized batches, and passages exceeding the
model's token limit; it never silently truncates text. Outputs are checked for
count, dimension, finite values, and nonzero vectors.

Inference is synchronous CPU work with two ONNX threads by default. The worker
uses bounded blocking work outside database transactions. Future API integration
must do the same and implement request deadlines/backpressure. A Tokio
timeout alone cannot stop CPU inference that has already started. Answer/vision
providers and their network timeouts are separate work; #26 is not complete.

## Models to compare

| Setting | Model | Approximate parameters | Vector size | Input limit |
|---|---|---|---|---|
| `bge-small-en-v1.5` (default) | BAAI/bge-small-en-v1.5 | 33 million | 384 | 512 tokens |
| `all-MiniLM-L6-v2` | sentence-transformers/all-MiniLM-L6-v2 | 23 million | 384 | 256 tokens |

Both are English-focused compact models. Parameter count, vector size, and input
limit describe different things; none alone guarantees retrieval quality.
The models and their ONNX exports are listed in [FastEmbed's model list](https://github.com/Anush008/fastembed-rs#supported-models).
See also the [BGE model card](https://huggingface.co/BAAI/bge-small-en-v1.5)
and [MiniLM model card](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2).

Ollama is a separate model service reached over HTTP. This implementation calls
an ONNX model inside the Rust process. Both can run locally without per-request
API charges; they have different deployment and resource-management trade-offs.
The earlier Qwen/Ollama laptop demo is not the production integration and was
not a controlled speed comparison with these CPU models.

## Run against the real UMass seed

From `backend/`, with Rust and native build tools installed:

```sh
export EMBEDDING_MODEL=bge-small-en-v1.5
export EMBEDDING_CACHE_DIR="$PWD/.cache/pintrail-embeddings"
export EMBEDDING_THREADS=2
cargo run -p pintrail-embeddings --example umass_search --locked

EMBEDDING_MODEL=all-MiniLM-L6-v2 \
  cargo run -p pintrail-embeddings --example umass_search --locked
```

The example also loads `.env`, but explicitly exported variables take priority.
The first model load downloads public ONNX/tokenizer files from Hugging Face;
artifact text is processed on this machine. Keep the cache persistent and
outside Git. Later deployment should provision model files before accepting
work, pin their verified artifact revision/digest, and verify offline startup.
The loaded provider now records its actual cached snapshot revision.
`pipeline_version` currently records the library/preprocessing configuration;
it is **not** a cryptographic identity for mutable remote model files.

The demo reads all 58 curated records directly from `seeds/umass/artifacts.toml`.
It prepares paragraph passages with names and ancestor context, embeds eight
fixed example queries, ranks artifacts by their best passage's cosine similarity,
and prints JSON containing top-three matches, model metadata, timings, and peak
process RSS on Linux. Topic hits are labeled with `is_topic`. The demo also
checks that an oversized input is rejected. It does not access a database or
production and does not implement public permission filtering.

These are hand-written smoke-test cases, not a representative ranking benchmark.
Compare repeat runs with a warm cache and the same thread count/build mode.
The figures describe the machine running the test, not the production VM; RSS
includes runtime/tokenizer overhead. Use collector questions before making a
final quality choice. Neither eight expected hits nor a similarity score is
a confidence guarantee. Seed facts still need the review documented in the seed README.

### Initial local measurements (2026-10-21)

Warm-cache runs completed with networking disabled (`unshare -n`) on the
developer's Ryzen 9 6900HS laptop, Ubuntu/WSL, debug build, CPU only, two model
threads. The same 58 records produced 132 paragraph passages and eight questions.

| Model | Expected artifact first | Expected artifact in top 3 | Embed 132 passages | Embed 8 questions (batch) | Peak process RSS |
|---|---|---|---|---|---|
| BGE-small | 7/8 | 8/8 | 5.08 s | 112 ms | 237 MiB |
| MiniLM | 6/8 | 8/8 | 3.52 s | 62 ms | 180 MiB |

Warm model initialization took 2.09 s and 1.20 s respectively. These are one-run
observations, not server sizing guarantees or measured API response times.
BGE-small remains the provisional default for its longer input allowance and
these initial results; MiniLM is a smaller alternative worth testing with real
collector queries. The small fixture does not establish a statistically reliable
quality difference. Do not compare these CPU figures directly with the earlier
GPU-backed Qwen/Ollama test.

## Tests

```sh
cargo test -p pintrail-embeddings --locked
cargo clippy -p pintrail-embeddings --all-targets --locked -- -D warnings
```

Tests do not initialize a real model, fetch weights, or call a provider service.
Building dependencies initially can download crates and ONNX Runtime binaries.
Other crates can enable `test-provider` for integration tests; the substitute is
not selectable through production model configuration and has no semantic meaning.

## Next integration work

- Add tags, linked topic names and per-link notes to prepared text and refresh triggers.
- Implement lexical/hybrid retrieval and the public search handler.
- Implement production passage splitting for longer collector descriptions.
- Expand relevance evaluation with independently collected user questions.
- Agree and test the pgvector database-image rollout before production enablement.

## Persistent search laboratory

The `postgres` feature adds real PostgreSQL vector storage and persistent
model-aware embedding jobs. The worker's `semantic-search` feature connects them
to text preparation. Both continuous operation and an explicit one-batch command
are available below. The HTTP search endpoint is not implemented yet.

The tables have different jobs:

| Table | What it holds | Why it exists |
|---|---|---|
| `artifacts` | The original names, descriptions, locations and approval states | Studio's source data |
| `search_documents` | Prepared searchable text and its content hash | Gives indexing a stable source to read |
| `search_index_jobs` | Work waiting, running, completed or failed | Lets the worker process changes without delaying an API request |
| `search_embeddings` | A document's vector plus its model identity and source hash | Saves model output so search can reuse it |
| `search_embedding_jobs` | Model-specific work, attempts, claim tokens and errors | Lets model work retry independently from preparing text |

An embedding is a list of 384 floating-point numbers for the default BGE model.
It is not a generated description, a trained model, or a collection of keywords.
The already-trained model calculates the list locally from text. PostgreSQL's
`vector` type stores that list; pgvector's cosine-distance operator compares it
with the list calculated for a question. Smaller distance means closer matches.
The score returned here is `1 - distance`, not a probability of correctness.

The model fingerprint covers name, ONNX repository, actual downloaded snapshot
revision, dimension, token limit and preprocessing pipeline version. It prevents
mixing incompatible vectors even when two models both produce 384 numbers.
The source hash identifies the exact prepared text that produced a vector.
Changing a document hash removes its saved vectors. A short row lock and hash
check reject results calculated from a source that changed during inference.
Inference runs before the database write transaction, never while holding it open.

`src/storage.rs` contains the important operations:

- `migrate`: explicitly installs the opt-in vector schema.
- `documents`: reads the text prepared by the existing worker.
- `save`: validates model identity and vector shape, checks the source is still
  current, then saves one row per document/model combination.
- `search`: compares a question vector with saved vectors from the same model.
  It requires current source hashes and `artifact_is_published` before applying
  the result limit. Drafts and children of unpublished parents cannot appear.

Vectors are bound as text parameters and cast to `vector` in PostgreSQL. They are
never interpolated into SQL strings. No new Rust pgvector driver is necessary.
Search is an exact cosine scan, appropriate for checking this small dataset.
There is no approximate HNSW/IVFFlat index yet. Results are document-level;
future multiple passages will need artifact grouping before limiting results.

### Database isolation

`backend/compose.search-lab.yml` is a standalone Compose project. Its database
is `pintrail_search_lab` at `127.0.0.1:5434`, with a separate Docker volume
`pintrail-search-lab_search_pgdata`. The existing development database stays at
port 5433. Never attach this Debian image to the old Alpine database volume.

The candidate image combines PostgreSQL 17, pgvector 0.8.7 and PostGIS packages.
The local build was checked with PostGIS 3.6.4. PostGIS is available for the
separate map work; this feature does not change artifact location columns.
The base image tag is versioned; apt-installed PostGIS packages are not pinned.
An agreed production image should pin its tested versions/digest.

The new migration lives under this crate's `migrations/`, **outside the normal
API migrations**. It runs only when explicitly invoked. This deliberately keeps
existing production and ordinary development databases from requiring pgvector
before the image rollout is reviewed. After that review it can be promoted into
the normal migration sequence. The lab is a CLI exercise, not a replacement
database for running the normal API against this intermediate migration layout.

### Running the lab from WSL

From `backend/`, with Rust installed and Docker access:

```sh
sudo docker compose -f compose.search-lab.yml up -d --build --wait
export SEARCH_DATABASE_URL=postgres://pintrail:pintrail@127.0.0.1:5434/pintrail_search_lab
export EMBEDDING_CACHE_DIR=/mnt/d/Software/Pintrail/.local-tools/fastembed-models

cargo run -p pintrail-embeddings --features postgres --example postgres_search -- init
DATABASE_URL="$SEARCH_DATABASE_URL" cargo run -p pintrail-worker -- index-search-text-once
cargo run -p pintrail-embeddings --features postgres --example postgres_search -- index
cargo run -p pintrail-embeddings --features postgres --example postgres_search -- search 'Where can I buy secondhand furniture?'
cargo run -p pintrail-embeddings --features postgres --example postgres_search -- compare 'Where can I buy secondhand furniture?'
```

`init` applies the current core schema, the opt-in embedding schema and the
58-artifact UMass development seed. The development seed is approved; this does
not approve or alter production data. `index-search-text-once` processes at most
1000 ready text jobs using the real existing worker, without requiring S3.
`index` calculates vectors for the prepared documents and saves them. It is an
explicit lab backfill, not yet an efficient missing/stale-only worker backfill.

`search` calculates only the question vector and reads saved document vectors.
`compare` additionally recalculates document vectors in Rust and checks the same
top three and cosine scores against PostgreSQL (tolerance 0.00001). Both use
full canonical documents, so the comparison is between identical inputs. The
earlier in-memory demo above uses 132 paragraph passages and is a separate
retrieval experiment; its quality numbers must not be attributed to this lab.
Oversized full documents are rejected rather than silently truncated. Production
chunking is still needed for longer collector submissions.

The example requires `SEARCH_DATABASE_URL` and checks the database name; it
never falls back to the normal `.env` database. Its single-thread Tokio runtime
is a sequential lab runner. The server integration must use bounded blocking
inference rather than copying synchronous calls into an async HTTP handler.

To check persistence, run `search`, restart only this lab's Postgres service,
then run `search` again without `index`. The named volume preserves the saved
vectors across restarts. Removing the volume with `down -v` destroys lab data.

### Verification and rollout boundary

The October 22 local setup backed up the original development database with
`pg_dump -Fc` and restored it into a separate database on the candidate image.
Original/restored artifact, document and job counts matched (one each). The
backup is private and ignored under `.local-tools/backups/`. The original
database was not migrated. Its old local migration-version history differs
from current main; the restore check does not attempt to reconcile that history.
A production backup/restore test still needs the real deployment's data and
environment before changing the server image.

The lab then saved 58 full-document BGE vectors. All eight sample questions
returned identical top-three document IDs and matching cosine scores in
PostgreSQL and Rust. Restarting the lab container preserved all 58 vectors;
a fresh process searched them without re-embedding documents. A second logical
backup restored into another database with identical vector values, model
fingerprints and source hashes. These checks verify plumbing and persistence,
not ranking quality on unseen questions.

Database tests use temporary SQLx databases, a deterministic three-number fixture,
and no downloaded model or paid API:

```sh
DATABASE_URL="$SEARCH_DATABASE_URL" cargo test -p pintrail-embeddings --features postgres --locked
cargo clippy -p pintrail-embeddings --features postgres --all-targets --locked -- -D warnings
```

They check public ranking before limits, private ancestors, model-version
isolation, stale-source rejection, invalid-vector rejection, idempotent saves
and cascading deletion. The dedicated Search storage workflow runs these tests
with a pgvector-enabled PostgreSQL image. Ordinary workspace tests remain usable
with the existing plain PostgreSQL image.

## Automatic embedding worker (2026-10-23)

The old manual `postgres_search index` remains a diagnostic command. For normal
local use, run the continuous worker instead:

```sh
# From backend/. The lab must already be initialized with postgres_search init.
export DATABASE_URL=postgres://pintrail:pintrail@127.0.0.1:5434/pintrail_search_lab
export EMBEDDING_CACHE_DIR=/mnt/d/Software/Pintrail/.local-tools/fastembed-models
export EMBEDDING_MODEL=all-MiniLM-L6-v2
cargo run -p pintrail-worker --features semantic-search -- search-loop
```

This command needs PostgreSQL and model files, not S3, the web API, or a paid key.
It loads the model once, processes up to 64 text jobs and 64 embedding jobs per
cycle, then waits two seconds. Leave it running to process future edits. Ctrl+C
stops it. `index-search-once` does one batch and exits; run repeatedly if a larger
backfill exceeds that batch. While `search-loop` is running, no separate manual
embedding command is needed.

The full media worker also supports this integration when compiled with
`semantic-search` and configured with `SEARCH_EMBEDDINGS_ENABLED=true`. Both are
opt-in. Default builds do not link the model into the worker, load/download
weights, or require the new database tables. Schema installation remains explicit;
the worker does not automatically migrate a production database.

The processing sequence is:

1. Existing artifact triggers add text-index jobs.
2. Existing text indexing updates the canonical document only when its hash changes.
3. The embedding poll discovers documents without a current vector for its model.
   It persists model-specific jobs, including an initial backfill on a model change.
4. A worker claims one ready job with `SKIP LOCKED` and a unique claim token.
5. `spawn_blocking` computes its embedding outside any database transaction.
   One model call at a time per indexer bounds CPU work.
6. A short transaction checks the claim and source hash, saves the vector and
   completes the job together. An outdated result is discarded.

Unchanged text is skipped, and missing vectors can be repaired. Switching the
model setting and restarting queues its missing vectors; old model vectors are
never mixed into new-model searches. Transient inference errors retry up to
three attempts with backoff. Invalid/oversized input fails without repeated CPU
work. Editing its source resets the job. Abandoned claims recover after five
minutes; expired workers cannot overwrite the replacement worker's result.
Processed jobs older than seven days are pruned in batches of 100. Failed jobs
remain in `search_embedding_jobs` with their error; admin UI integration for
these separate embedding jobs is not implemented yet.

Text work and model work use separate job tables because model failure or a
model change does not mean the prepared text has to be rebuilt. The existing
deployed text indexer remains usable without the optional embedding component.

Six deterministic database tests check automatic backfill, unchanged text,
missing-vector repair, source edits/deletion, model revisions, competing workers,
retry/permanent errors, source changes during inference and expired claim fencing.
A live check also ran the actual continuous worker and real BGE model against a
temporary fictional artifact: creation generated its vector, a changed description
changed both hash and vector, a no-op edit preserved it, and soft deletion removed
its document and vectors. The original 58 vectors stayed unchanged; the fictional
artifact was removed after verification.

## Expanded model evaluation (2026-10-23)

`examples/evaluate_search.rs` evaluates 32 supported questions: the original eight
development examples and 24 additional cases. The additional cases include
paraphrases, exact names/facts, distinctions between similar artifacts, concepts
and a typo. Expected keys are checked against the seed; the 24 additional positive
cases include supporting text checked against the descriptions before inference.
Four unsupported queries test behavior for absent content. Expected answers and
evidence are never supplied to the embedding model. No training/fine-tuning occurs.

Full-document inputs follow the worker's current canonical text format. A
`--passages` flag also compares the separate paragraph experiment fairly across
both models. Each artifact receives its best passage score in that mode.

| Input form | Method | Expected first | Expected in top 3 |
|---|---|---|---|
| 58 full documents | BGE-small | 30/32 | 31/32 |
| 58 full documents | MiniLM | 32/32 | 32/32 |
| 58 full documents | Simple word-overlap baseline | 29/32 | 31/32 |
| 132 paragraphs | BGE-small | 30/32 | 32/32 |
| 132 paragraphs | MiniLM | 29/32 | 32/32 |
| 132 paragraphs | Simple word-overlap baseline | 25/32 | 30/32 |

At this 32-question stage, MiniLM looked promising
for the current full-document pipeline. BGE retains the longer 512-token input
limit versus MiniLM's 256. Both process all 58 current documents without truncation.
Keep models selectable; the original configuration default remains BGE, while the
command above explicitly selects MiniLM for evaluating this new evidence.
The larger follow-up below supersedes any apparent clear advantage. These
authored cases are small and often contain informative names/facts, which
also helps word matching. They are not independent collector queries and do not
establish general superiority or production accuracy.

Inspect failures rather than only totals: BGE's full-document run ranked the
ValleyBike topic sixth for borrowing a bike at one station and returning it at
another, preferring individual station records. It also ranked a general LEED
topic ahead of the basketball training building. Do not adjust expected labels
just to improve the measured score.

Both models still return nearest candidates for unsupported questions; for
example, there is no campus fusion reactor in the corpus. BGE assigned one such
nearest candidate about 0.655 similarity. A similarity score is not a confidence
probability and does not establish that the requested artifact exists. A future
no-close-match policy needs additional independently labeled negative queries;
this evaluator deliberately does not invent a cutoff from four examples.

Run the evaluations from backend/ with a persistent model cache:

```sh
EMBEDDING_MODEL=bge-small-en-v1.5 cargo run -p pintrail-embeddings --example evaluate_search --locked
EMBEDDING_MODEL=all-MiniLM-L6-v2 cargo run -p pintrail-embeddings --example evaluate_search --locked
# Optional paragraph comparison; use the same flag for both models.
EMBEDDING_MODEL=all-MiniLM-L6-v2 cargo run -p pintrail-embeddings --example evaluate_search --locked -- --passages

# Evaluate the actual saved PostgreSQL vectors, embedding only the questions:
export SEARCH_DATABASE_URL="$DATABASE_URL"
EMBEDDING_MODEL=all-MiniLM-L6-v2 cargo run -p pintrail-embeddings --features postgres --example postgres_search --locked -- evaluate
```

Output includes per-question ranks/top matches, category breakdowns, hit@1,
hit@3, mean reciprocal rank, query embedding latency and Linux peak RSS. Timing
numbers are local debug-build observations, not server benchmarks. The simple
word-overlap baseline is not PostgreSQL full-text or hybrid retrieval.

### Follow-up: 32 harder questions and direct length probes

`harder_cases.json` adds 16 indirect paraphrases, eight questions with distracting
context, four spelling-corruption cases, and four distinctions between similar
artifacts. Expected keys were fixed before running the models and were not changed
to improve results. Evaluation now contains 64 supported queries plus the same
four unsupported queries. These are still authored cases, not independent user data.

| Full-document retrieval | Expected first | Expected in top 3 |
|---|---|---|
| BGE, all 64 | 58/64 | 62/64 |
| MiniLM, all 64 | 57/64 | 63/64 |
| BGE, new harder 32 only | 28/32 | 31/32 |
| MiniLM, new harder 32 only | 25/32 | 31/32 |

The full-document counts were reproduced using the actual stored PostgreSQL
vectors. With paragraph inputs, BGE scored 55/64 first and 63/64 top-three;
MiniLM scored 53/64 first and 63/64 top-three. Paragraph splitting therefore did
not consistently improve retrieval. Neither model is a clear general winner.
The configured BGE default remains reasonable provisionally for its longer input
allowance, without claiming that allowance caused the ranking difference.

Both models missed the expected Durfee artifact in their top three for “I need
a little tropical escape between lectures when everything outdoors is freezing.”
BGE ranked it fourth and MiniLM seventh. This failure stays in the test fixture.

`examples/input_limits.rs` directly exercises both query and document limits with
synthetic repeated words. This is a capacity check, not an accuracy benchmark.
Both accepted a 20-word input. MiniLM accepted 254 repetitions of `water` (including
two special tokens: 256 tokens total), rejected 255 (257 tokens), and rejected 300
(302 tokens). BGE accepted 300 repetitions for questions and documents. BGE's
question prefix also consumes space: 510 repetitions were accepted as a document
but rejected as a question (520 tokens after its instruction and special tokens).
Other words can split into more pieces; these word counts are not universal limits.

A token limit is the amount of text accepted per input, not a model quality score
or a measure of its parameter count. Questions and documents are embedded
separately; a question does not share its allowance with all retrieved documents.
The worker currently rejects oversized documents rather than truncating them;
production chunking is still a separate search task.

```sh
EMBEDDING_MODEL=bge-small-en-v1.5 cargo run -p pintrail-embeddings --example input_limits --locked
EMBEDDING_MODEL=all-MiniLM-L6-v2 cargo run -p pintrail-embeddings --example input_limits --locked
```
