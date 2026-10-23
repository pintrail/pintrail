//! Explicit real-model evaluation; not an assertion that the small authored
//! fixture represents real users. No expected IDs or evidence enter the model.
use pintrail_embeddings::{Config, EmbeddingProvider, InputKind, LocalProvider};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    time::Instant,
};

#[derive(Deserialize)]
struct Seed {
    artifact: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    key: String,
    name: String,
    description: String,
    parent: Option<String>,
}
#[derive(Deserialize)]
struct Case {
    #[serde(default)]
    id: String,
    #[serde(default)]
    category: String,
    question: String,
    expected: Vec<String>,
    evidence: Option<String>,
}
#[derive(Default, serde::Serialize)]
struct Metric {
    count: usize,
    hit_1: usize,
    hit_3: usize,
    mrr: f64,
}
impl Metric {
    fn add(&mut self, rank: Option<usize>) {
        self.count += 1;
        if let Some(rank) = rank {
            self.hit_1 += usize::from(rank == 1);
            self.hit_3 += usize::from(rank <= 3);
            self.mrr += 1.0 / rank as f64;
        }
    }
    fn finish(&mut self) {
        if self.count > 0 {
            self.mrr /= self.count as f64;
        }
    }
}
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn words(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .filter(|w| {
            !matches!(
                w.as_str(),
                "the"
                    | "and"
                    | "where"
                    | "which"
                    | "what"
                    | "for"
                    | "from"
                    | "with"
                    | "that"
                    | "this"
                    | "can"
                    | "its"
                    | "there"
                    | "instead"
                    | "campus"
                    | "building"
            )
        })
        .collect()
}
fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let norm = |v: &[f32]| v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    dot / (norm(a) * norm(b))
}
fn ranked(scores: &[f64]) -> Vec<usize> {
    let mut order: Vec<_> = (0..scores.len()).collect();
    order.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]).then(a.cmp(b)));
    order
}
fn main() -> Result<(), Box<dyn Error>> {
    let seed: Seed = toml::from_str(include_str!("../../../seeds/umass/artifacts.toml"))?;
    let mut cases: Vec<Case> = serde_json::from_str(include_str!("umass_queries.json"))?;
    for (i, c) in cases.iter_mut().enumerate() {
        c.id = format!("development-{}", i + 1);
        c.category = "development".into();
    }
    let extra: Vec<Case> = serde_json::from_str(include_str!("semantic_cases.json"))?;
    cases.extend(extra);
    cases.extend(serde_json::from_str::<Vec<Case>>(include_str!(
        "harder_cases.json"
    ))?);
    let lookup: HashMap<_, _> = seed.artifact.iter().map(|a| (a.key.as_str(), a)).collect();
    let mut ids = HashSet::new();
    for c in &cases {
        if !ids.insert(&c.id) || c.question.trim().is_empty() {
            return Err("invalid/duplicate evaluation case".into());
        }
        if c.expected.is_empty() != (c.category == "unsupported") {
            return Err("only unsupported cases can lack expected matches".into());
        }
        for key in &c.expected {
            let artifact = lookup
                .get(key.as_str())
                .ok_or("expected key missing from seed")?;
            if let Some(evidence) = &c.evidence {
                if !normalize(&artifact.description).contains(&normalize(evidence)) {
                    return Err(format!("evidence does not occur in {} for {}", key, c.id).into());
                }
            }
        }
    }
    let passages = std::env::args().any(|a| a == "--passages");
    let mut texts = Vec::new();
    let mut owners = Vec::new();
    for (i, a) in seed.artifact.iter().enumerate() {
        let mut parents = Vec::new();
        let mut parent = a.parent.as_deref();
        while let Some(key) = parent {
            if parents.len() >= seed.artifact.len() {
                return Err("cyclic parents".into());
            }
            let p = lookup.get(key).ok_or("missing parent")?;
            parents.push(p.name.as_str());
            parent = p.parent.as_deref();
        }
        parents.reverse();
        let mut prefix = format!("Name: {}\nKind: other", a.name.trim());
        if !parents.is_empty() {
            prefix.push_str(&format!("\nParents: {}", parents.join(" > ")));
        }
        let description = a.description.replace("\r\n", "\n");
        let chunks: Vec<_> = if passages {
            description
                .split("\n\n")
                .filter(|p| !p.trim().is_empty())
                .collect()
        } else {
            vec![description.as_str()]
        };
        for chunk in chunks {
            texts.push(format!("{}\nDescription: {}", prefix, chunk.trim()));
            owners.push(i);
        }
    }
    let config = Config::from_env()?;
    let start = Instant::now();
    let mut model = LocalProvider::new(config)?;
    let load_ms = start.elapsed().as_millis();
    let start = Instant::now();
    let mut vectors = Vec::new();
    for batch in texts.chunks(32) {
        vectors.extend(model.embed(
            &batch.iter().map(String::as_str).collect::<Vec<_>>(),
            InputKind::Document,
        )?);
    }
    let index_ms = start.elapsed().as_millis();
    let lexical: Vec<_> = texts.iter().map(|t| words(t)).collect();
    let mut semantic_metrics: BTreeMap<String, Metric> = BTreeMap::new();
    let mut lexical_metrics: BTreeMap<String, Metric> = BTreeMap::new();
    let mut results = Vec::new();
    let mut latency = Vec::new();
    for c in &cases {
        let start = Instant::now();
        let query = model.embed(&[&c.question], InputKind::Query)?.remove(0);
        latency.push(start.elapsed().as_secs_f64() * 1000.0);
        let mut semantic = vec![f64::NEG_INFINITY; seed.artifact.len()];
        let mut literal = vec![0.0f64; seed.artifact.len()];
        let query_words = words(&c.question);
        for ((v, owner), terms) in vectors.iter().zip(&owners).zip(&lexical) {
            semantic[*owner] = semantic[*owner].max(cosine(&query, v));
            literal[*owner] = literal[*owner].max(query_words.intersection(terms).count() as f64);
        }
        let order = ranked(&semantic);
        let baseline = ranked(&literal);
        let rank = |order: &[usize]| {
            order
                .iter()
                .position(|i| c.expected.contains(&seed.artifact[*i].key))
                .map(|n| n + 1)
        };
        if !c.expected.is_empty() {
            for group in ["all_supported", c.category.as_str()] {
                semantic_metrics
                    .entry(group.into())
                    .or_default()
                    .add(rank(&order));
                lexical_metrics
                    .entry(group.into())
                    .or_default()
                    .add(rank(&baseline));
            }
        }
        let matches:Vec<_>=order.iter().take(3).map(|i|json!({"key":seed.artifact[*i].key,"name":seed.artifact[*i].name,"score":semantic[*i]})).collect();
        results.push(json!({"id":c.id,"category":c.category,"question":c.question,"expected":c.expected,
            "supporting_text":c.evidence,"first_expected_rank":rank(&order),"word_overlap_first_expected_rank":rank(&baseline),
            "unsupported":c.expected.is_empty(),"matches":matches}));
    }
    for metric in semantic_metrics
        .values_mut()
        .chain(lexical_metrics.values_mut())
    {
        metric.finish();
    }
    latency.sort_by(f64::total_cmp);
    let percentile = |p: f64| latency[((latency.len() - 1) as f64 * p).round() as usize];
    let peak = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"model":model.model_info(),"device":"CPU",
        "mode":if passages{"paragraph_passages"}else{"full_canonical_documents"},"artifact_count":seed.artifact.len(),"input_count":texts.len(),
        "supported_queries":cases.iter().filter(|c|!c.expected.is_empty()).count(),
        "unsupported_queries":cases.iter().filter(|c|c.expected.is_empty()).count(),
        "development_queries":cases.iter().filter(|c|c.category=="development").count(),
        "additional_supported_queries":cases.iter().filter(|c|!c.expected.is_empty() && c.category!="development").count(),
        "semantic":semantic_metrics,"word_overlap_baseline":lexical_metrics,"model_load_ms":load_ms,"document_embedding_ms":index_ms,
        "query_embedding_p50_ms":percentile(0.5),"query_embedding_p95_ms":percentile(0.95),"peak_rss_kib":peak,
        "unsupported_behavior":"Nearest matches are returned even for unsupported queries; no abstention threshold is configured.",
        "limitations":"Authored fixtures, not independent collector questions. No model tuning. Word overlap is a simple baseline, not PostgreSQL full-text search.","results":results}))?
    );
    Ok(())
}
