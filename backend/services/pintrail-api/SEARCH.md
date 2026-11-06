# Search API

`GET /search?q=...&limit=...&mode=...` returns ranked, approved artifacts using
keyword, semantic or hybrid retrieval (#28). Semantic remains the default while
the hybrid implementation is evaluated. Location ranking is separate follow-up
work. No answer-generation provider or paid API key is involved.

## Request and response

Use an existing reader bearer token or author session cookie, as for `/artifacts`.
Both identity types receive the same approved-content search results; this is
not a draft-preview endpoint. An unverified reader can browse, following the
existing API's read permissions.

```sh
curl --get http://localhost:8080/search \
  -H "Authorization: Bearer $PINTRAIL_READER_TOKEN" \
  --data-urlencode 'q=Where can I buy secondhand clothes?' \
  --data-urlencode 'mode=hybrid' \
  --data-urlencode 'limit=3'

# Or use an author cookie obtained by logging in to this same API/database:
curl --get http://localhost:8080/search \
  -b /tmp/pintrail-author.cookies \
  --data-urlencode 'q=A tropical escape between lectures' \
  --data-urlencode 'limit=3'
```

The JSON response has this shape (illustrative values):

```json
{
  "query": "Where can I buy secondhand clothes?",
  "mode": "semantic",
  "model": "BAAI/bge-base-en-v1.5",
  "score_kind": "cosine_similarity",
  "count": 1,
  "results": [
    {
      "artifact_id": "00000000-0000-4000-8000-000000000001",
      "name": "Example thrift store",
      "is_topic": false,
      "snippet": "A shop selling donated clothing and household items.",
      "score": 0.7,
      "semantic_score": 0.7
    }
  ]
}
```

`count` is the number returned, not a corpus-wide total. `score_kind` identifies
the mode-specific meaning of `score`; no score is a confidence probability.
`semantic_score` and `keyword_score` are included only when that branch supplied
the artifact. `model` is null in keyword mode because no query inference runs.
A snippet is the first 280 Unicode
characters of the current approved artifact description; it is not a generated
answer or a highlighted best passage. Topics are clearly marked with `is_topic`.
Document IDs, source hashes, author details and account data are not returned.

`q` is required, trimmed, nonempty and limited to 4096 UTF-8 bytes. The selected
model's token limit is checked in semantic/hybrid mode, including its query instruction
and special tokens. Oversized input is rejected, not truncated. `limit` defaults
to 10 and must be 1–20. Unknown modes/parameters are rejected.

## Retrieval modes

| Mode | Retrieval | Returned score |
|---|---|---|
| `keyword` | PostgreSQL English full-text search over prepared text; no embedding call | Full-text rank, with extra name weight |
| `semantic` (default) | Exact cosine comparison against compatible saved vectors | Cosine similarity |
| `hybrid` | Semantic-first ordering with selective keyword boosts, up to 50 artifacts per branch | Semantic-first fusion score |

Keyword search uses the existing names/descriptions/ancestor names/tags/topic
context in `search_documents.content`. A stored generated `search_terms` column
and GIN index follow changes automatically. No collector field, separate
database or new model is required. English normalization handles word forms,
but does not provide synonym matching or typo correction. Bare terms use AND;
quotes require a phrase, `OR` selects alternatives and `-term` excludes a term.
These are [PostgreSQL web-search query semantics](https://www.postgresql.org/docs/17/textsearch-controls.html).

Exact whole-artifact names take priority in keyword/hybrid results, ignoring
case, repeated whitespace and optional enclosing quotes. Other lexical matches
receive body rank plus twice the name-field rank. Keyword results can include
prepared, approved artifacts whose embeddings are not ready yet; model-version
and embedding-source checks still apply to the vector branch independently.

The current hybrid policy is `semantic-first-v1`. It starts with semantic result
order, using `1 / (60 + position)` as an ordering score. Ordinary conversational
queries do not receive an automatic second vote for appearing in the keyword
list. Weak lexical-only hits are appended after the semantic candidates in their
own lexical order.

Keyword contributions are enabled for explicit phrases/exclusions, when the
semantic candidate list is empty, or for selective short lookups. A short lookup
has at most three whitespace-separated words, does not start with a common
English question/auxiliary word, and matches one to three **approved** artifacts
in the lexical branch. These are conservative, provisional heuristics; they do
not understand every query's intent or establish calibrated confidence.

When enabled, lexical support adds `1 / (60 + lexical_position)`, following
[reciprocal-rank fusion](https://cormack.uwaterloo.ca/cormacksigir09-rrf.pdf).
Exact whole names retain priority in all hybrid cases, followed by numeric
score and stable artifact-ID ties. `score_kind` is `semantic_first_fusion`.
`keyword_score` can be present even when its match did not receive a boost;
it records lexical evidence, not necessarily a numeric contribution.
The earlier equal-vote rule and results remain documented below. No second
model, learned classifier, or per-artifact exception list is involved.

For hybrid queries containing quotes or `-term`, the full-text query is also
required for vector candidates **before** their candidate limit. This prevents
semantic results from bypassing explicit phrases/exclusions. Phrase matching
uses PostgreSQL's normalized English terms, not byte-for-byte substring matching.
For example, `"solar panels" -parking` requires the phrase and excludes parking.
Semantic-only mode treats the supplied string as model input, not Boolean syntax.

| Status | Meaning |
|---|---|
| 200 | Ranked results; an empty list is possible if nothing matches or compatible indexed documents are unavailable |
| 400 | Missing/invalid input, unsupported parameters, or an over-limit question |
| 401 | Missing, expired or invalid login |
| 429 | More than 60 valid search attempts per account in a rolling minute |
| 503 | Search disabled, busy, or timed out; retry later |
| 500 | Internal failure; details stay in server logs |

Unsupported questions can still return nearest candidates. No calibrated
no-match threshold is implemented. Search returns matching artifacts, not a
claim that the requested thing exists or a written answer.

## Enable locally

The database-image rollout remains opt-in. Search does not install pgvector or
run the embedding migrations automatically. Follow the
[isolated lab guide](../../libs/pintrail-embeddings/README.md) first; never point
the candidate database image at an existing incompatible database volume.

For an API already configured with its normal PostgreSQL, object-storage and
mailer settings, set:

```sh
export SEARCH_ENABLED=true
export EMBEDDING_MODEL=bge-base-en-v1.5
export EMBEDDING_CACHE_DIR=/path/to/persistent/model-cache
cargo run -p pintrail-api --features semantic-search --locked
```

For the isolated lab, explicitly set `DATABASE_URL` to
`postgres://pintrail:pintrail@127.0.0.1:5434/pintrail_search_lab` and use its model
cache. The lab already contains 58 seed artifacts and their BGE-base vectors.
Its accounts/sessions are separate from the ordinary development database.
The normal API object store must also be available: the API validates it at
startup even though search itself does not read files.
The default container build is unchanged. A container rollout will need an image
built with the feature, the explicit environment setting and a persistent model
cache; setting a variable in a Compose `.env` alone does not configure all of that.

Migration `20261105000001_search_full_text.sql` adds the generated English word
index to the normal core schema. API startup applies it automatically; CLI lab
users should run `postgres_search init` to bring the lab schema up to date.
Existing prepared text is indexed without regenerating embeddings. The stored
column/index build takes a table lock, so plan the normal reviewed migration
rollout for a large production corpus. This work applied it only to the isolated
lab and disposable test databases.

`SEARCH_ENABLED` defaults to false. Enabling it without the build feature or
embedding schema fails clearly at startup. Operator commands such as `migrate`
and `create-author` do not load the model. The API loads one selected model once
before listening. The first cold-cache load downloads public model files;
question text is processed locally. Use the same model, snapshot and pipeline
as the embedding worker; a different model requires its own indexed vectors.

The API reads saved vectors and never embeds every artifact on a request.
Keyword-only requests skip inference, although the enabled search service still
loads its configured model once at startup for semantic/hybrid requests.
The background worker continues to own document preparation and vector updates.
Neither this endpoint nor its tests change Studio's collection fields.

## Request execution and bounds

1. Authenticate, validate input and apply the per-account rate limit.
2. Reserve one search slot. An occupied slot returns 503 instead of creating an
   unbounded inference queue. The API and worker are separate processes.
3. For semantic/hybrid, use `spawn_blocking` to embed only the question, keeping CPU work off Tokio's
   asynchronous request threads. No database connection or lock is held here.
4. Retrieve the selected branches in a read-only, repeatable-read transaction,
   so they see the same document/publication snapshot. Filter publication before
   each candidate limit, and model/source identity before vector limits. Group
   documents by artifact and combine branches before the response limit.
   Deleted/draft artifacts and children of unpublished ancestors cannot occupy
   visible candidate slots.
5. Return safe result fields and release the search slot.

There is a 10-second operation deadline and a PostgreSQL transaction-local
3-second statement timeout. A started blocking inference cannot be forcibly
cancelled by Tokio; it retains a reference to the concurrency permit until CPU
work actually ends, even if its HTTP request times out or is cancelled. That
prevents abandoned requests from creating overlapping model jobs.

The current worker still prepares one full canonical artifact document per
artifact. Retrieval groups documents into artifact results; actual attachment
processing, long-document chunking, approximate vector indexes and calibrated
abstention are separate work.

## Validation

```sh
cargo test --workspace --features pintrail-api/semantic-search,pintrail-worker/semantic-search --locked
cargo clippy --workspace --all-targets --features pintrail-api/semantic-search,pintrail-worker/semantic-search --locked -- -D warnings
```

Use an isolated PostgreSQL test server with pgvector for the feature tests.
The tests use the deterministic embedding provider and fetch no model weights
or external services. They cover reader/author authentication, public visibility
before the limit, model/source isolation, Unicode snippets, input errors, rate
limits, safe errors, and inference timeout/cancellation ownership. The usual
default build remains usable without enabling semantic search.

On November 4, the full API was also exercised over real HTTP against the
isolated 58-artifact lab using the cached BGE-base model. Across the unchanged
64 supported questions, the expected artifact was first for 62 and in the top
three for all 64. Four unsupported questions were observed separately and still
returned nearest candidates. Real missing-login and oversized-token requests
returned 401 and 400 respectively. Temporary test accounts and services were
removed afterward; the original development database was not used.

One local Ryzen 9 6900HS/WSL debug CPU run measured complete HTTP request latency
of 82 ms p50 and 135 ms p95, including authentication, question embedding and
retrieval. These authored cases and one-run timings do not establish production
accuracy or performance. The 124 automated tests passed with the API/worker
features enabled; strict lint checks also passed with features enabled and off.

### November 5: initial keyword/hybrid comparison

The unchanged 64 supported questions and four unsupported requests were run
through all three HTTP modes. Twelve additional, explicitly authored lexical
cases were frozen before measurement and reported separately. They cover exact
names, rare terms and phrases; they are not independent student validation.

| Mode | Original 64: first / top 3 | Added lexical 12: first / top 3 |
|---|---|---|
| Semantic | 62 / 64 | 9 / 10 |
| Keyword | 5 / 5 | 12 / 12 |
| Hybrid | 60 / 63 | 12 / 12 |

For `electrochromic`, semantic search missed Worcester Commons in its top three;
keyword and hybrid put it first. Conversely, hybrid promoted the general EV
charging topic above the requested Campus Center garage charger. An indirect
architecture question mentioning "not gardens or solar panels" also regressed:
the web-search lexical parser and rank overlap do not understand conversational
negation the way semantic matching can. Plain keyword AND matching returns few
results for long conversational questions; this mode is most useful for specific
terms and explicit search syntax.

Semantic therefore remains the default. Hybrid is an explicit mode, not a proven
universal improvement. No labels, weights or rank constants were changed after
seeing these outcomes. No independent collector questions were available.

Local debug HTTP p50/p95 measurements were 79/120 ms for semantic, 11/15 ms for
keyword, and 84/122 ms for hybrid. These are one-run CPU observations across both
suites, not controlled server benchmarks. The full 130-test feature suite and
both lint configurations passed; after a query-plan optimization, all affected
retrieval/API tests passed again, every recorded result order was reproduced,
and a usable GIN index path was verified without forcing the production planner.

The per-query comparison and fixture hashes are in
[`2026-11-05-hybrid.json`](../../libs/pintrail-embeddings/evaluation-results/2026-11-05-hybrid.json).
Reproduce the ranking comparison from `backend/`, with the initialized lab,
persistent cache and matching BGE-base vectors:

```sh
export SEARCH_DATABASE_URL=postgres://pintrail:pintrail@127.0.0.1:5434/pintrail_search_lab
export EMBEDDING_MODEL=bge-base-en-v1.5
export EMBEDDING_CACHE_DIR=/path/to/persistent/model-cache
cargo run -p pintrail-embeddings --features postgres --example postgres_search --locked -- evaluate-modes
```

This explicit CLI benchmark loads a real model; normal `cargo test` uses the
deterministic provider. CLI and HTTP aggregate metrics and every result ordering
were checked for agreement. The original `evaluate` command remains the original
semantic-only fixture for backward comparison. `evaluate-modes` now also reports
the refinement diagnostics below; compare the saved historical report when
reviewing the earlier equal-vote algorithm.

### Semantic-first refinement

The revised policy was developed in response to the two failures above. The
original 64 supported questions, 12 lexical cases, expected labels and corpus
were unchanged. Five additional authored diagnostic paraphrases were fixed
before the first refinement run and reported separately.

| Retrieval | Original 64: first / top 3 | Lexical 12: first / top 3 | Diagnostic 5: first / top 3 |
|---|---|---|---|
| Semantic | 62 / 64 | 9 / 10 | 4 / 4 |
| Initial hybrid (historical) | 60 / 63 | 12 / 12 | Not measured |
| Revised hybrid | 62 / 64 | 12 / 12 | 4 / 4 |

The garage charging station now remains ahead of the general EV topic. The
architecture question now keeps John W. Olver Design Building first, despite
mentioning gardens and solar panels. The short `electrochromic` lookup still
finds Worcester Commons first. These are fixes on development data, not proof
of general superiority.

The diagnostic question "Which artifact mentions electrochromic technology?"
still misses Worcester Commons in the top three in both semantic and hybrid
modes. English full-text AND matching returns no candidate for that complete
wording, while the semantic model also misses it. The new rule deliberately
does not claim to solve long questions containing rare terms. The word-count
and match-count limits are provisional and may need revisiting as the corpus
grows. Semantic remains the API default pending independent validation.

All 134 automated feature tests passed, including four new regressions covering
question ordering, conversational negation, selective versus broad lookups,
empty-vector fallback and long exact names. Both lint configurations passed.
The complete API was exercised over HTTP; CLI metrics and every result order
were checked against HTTP. No new model, schema, embedding generation or label
changes were needed for this refinement.

The initial report remains intact; revised per-query results and unchanged
fixture hashes are in
[`2026-11-05-hybrid-refined.json`](../../libs/pintrail-embeddings/evaluation-results/2026-11-05-hybrid-refined.json).
`postgres_search evaluate-modes` now emits `hybrid_ranking_version` and separate
`existing`, `lexical` and `diagnostics` groups. Requests using hybrid mode return
the updated `score_kind: semantic_first_fusion`; semantic and keyword score
kinds are unchanged.
