//! The Studio manual: how-to and reference pages for authors, inside the app.
//!
//! The pages are Markdown files in `studio/help/`, compiled into the binary,
//! so the manual always matches the Studio it ships with and needs no
//! separate site. [`MANUAL`] is the table of contents: sections, each with
//! its pages in reading order. Adding a page means writing the file and
//! adding one line there.
//!
//! Pages are for signed-in authors only (any role), like the rest of the
//! Studio. Each page is served three ways:
//!
//! * `/studio/help` and `/studio/help/{slug}` in the main area, with the
//!   manual's contents alongside (a full document on a direct visit);
//! * `?panel=1`, a compact version for the help panel that slides in from
//!   the right when an author clicks a "?" next to something, so they can
//!   read about it without leaving the form they're filling in.
//!
//! Markdown conventions, beyond the usual:
//!
//! * a blockquote that starts with **Example**, **Tip**, **Note**, or
//!   **Watch out** becomes a coloured callout;
//! * headings get ids from their text ("Good tags" → `#good-tags`), so a "?"
//!   can open a page at a particular section;
//! * links to `/studio/help/...` stay inside the manual (or the panel).

use std::collections::HashMap;
use std::sync::OnceLock;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use axum_extra::extract::cookie::CookieJar;
use minijinja::{context, Value};
use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Deserialize;
use serde_json::json;

use super::routes::{base_ctx, environment, render, respond, session_token};
use crate::authors::extractors::AuthenticatedAuthor;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub struct Page {
    pub slug: &'static str,
    pub title: &'static str,
    /// One sentence for the contents and search results.
    pub summary: &'static str,
    pub source: &'static str,
}

pub struct Section {
    pub title: &'static str,
    pub pages: &'static [Page],
}

macro_rules! page {
    ($slug:literal, $title:literal, $summary:literal) => {
        Page {
            slug: $slug,
            title: $title,
            summary: $summary,
            source: include_str!(concat!("help/", $slug, ".md")),
        }
    };
}

/// The table of contents, in reading order.
pub static MANUAL: &[Section] = &[
    Section {
        title: "Getting started",
        pages: &[
            page!("welcome", "Welcome to the Studio", "What the Studio is for, and the five things to do first."),
            page!("signing-in", "Signing in and your profile", "Your first sign-in, choosing a password, and filling in your profile."),
            page!("finding-your-way", "Finding your way around", "The sidebar, the main area, the buttons, and where help lives."),
            page!("who-can-do-what", "Who can do what", "Viewers, editors, and admins, and why you can change only what you made."),
        ],
    },
    Section {
        title: "Key ideas",
        pages: &[
            page!("artifacts", "Artifacts", "The things worth stopping for: what an artifact is and what it holds."),
            page!("nesting", "Artifacts inside artifacts", "How a building holds its roof, its rooms, and its systems."),
            page!("location", "Location", "The pin on the map, why it matters, and how an artifact can use its parent's."),
            page!("kinds", "Kinds", "Building, room, artwork, installation, rooftop, or other: what each one is for."),
            page!("tags", "Tags", "Themes people search by, and how to keep them consistent."),
            page!("topics", "Topics", "One shared page that many artifacts link to, like LEED certification."),
            page!("trails", "Trails", "A walk through artifacts in order, with a note at each stop."),
            page!("review-and-the-app", "Review, and what the app shows", "Draft, ready for review, approved: and why only approved artifacts reach phones."),
            page!("tags-kinds-topics", "Kinds, tags, topics, or nesting?", "Four ways to group artifacts, and how to choose between them."),
        ],
    },
    Section {
        title: "Working with artifacts",
        pages: &[
            page!("adding-an-artifact", "Adding an artifact", "Create it with its name and place, then fill it in."),
            page!("adding-inside", "Adding an artifact inside another", "Rooms, roofs, and systems that belong to a building."),
            page!("editing", "Editing an artifact", "Changing the details, moving it, and what happens to an approved one."),
            page!("writing-descriptions", "Writing a good description", "What to say, how long, and an example to follow."),
            page!("links", "Source links", "Adding the pages your facts come from, and putting them in order."),
            page!("media", "Photos and PDFs", "Uploading, what file types work, and taking photos on your phone."),
            page!("deleting", "Deleting and restoring", "What Delete removes, who can delete, and getting something back."),
            page!("history", "History", "Every change to an artifact, who made it, and when."),
        ],
    },
    Section {
        title: "Topics",
        pages: &[
            page!("creating-a-topic", "Creating a topic", "Starting a shared page, and what to write on it."),
            page!("linking-topics", "Linking artifacts to a topic", "Linking from either side, notes on a link, and adding a whole set at once."),
        ],
    },
    Section {
        title: "Trails",
        pages: &[
            page!("creating-a-trail", "Creating a trail", "Title, description, and who can see it."),
            page!("trail-stops", "Stops", "Adding stops, putting them in walking order, and notes at each."),
            page!("sharing-a-trail", "Sharing a trail", "Private, anyone with the link, or public."),
        ],
    },
    Section {
        title: "The map and review",
        pages: &[
            page!("map", "The map", "Every artifact on one map, with filters by kind, status, and tag."),
            page!("submitting-for-review", "Submitting for review", "Sending a finished artifact to your instructor, and what comes back."),
            page!("reviewing", "Reviewing (admins)", "The review queue, approving, and sending back with a note."),
        ],
    },
    Section {
        title: "On your phone",
        pages: &[
            page!("phone", "Using the Studio on your phone", "Adding artifacts on site: your location, the camera, and the menu."),
        ],
    },
    Section {
        title: "For admins",
        pages: &[
            page!("managing-authors", "Managing authors", "Adding people, roles, resetting passwords, suspending, and deleting."),
            page!("owners-and-recovery", "Owners and recovering deleted work", "Giving an artifact to someone else, and restoring a deleted one."),
            page!("moderation", "Comments and explorers' trails", "What the admin panel is for beyond authors."),
        ],
    },
    Section {
        title: "Reference",
        pages: &[
            page!("glossary", "Glossary", "Every term the Studio uses, in one place."),
            page!("troubleshooting", "Troubleshooting", "When something doesn't work the way you expect."),
        ],
    },
];

pub fn router() -> axum::Router<AppState> {
    Router::new()
        .route("/studio/help", get(index))
        .route("/studio/help/{slug}", get(page))
}

// --- rendering -----------------------------------------------------------------

struct Rendered {
    html: String,
    /// Second-level headings, for "On this page".
    outline: Vec<(String, String)>,
    /// Plain text, for search.
    text: String,
}

fn rendered() -> &'static HashMap<&'static str, Rendered> {
    static CACHE: OnceLock<HashMap<&'static str, Rendered>> = OnceLock::new();
    CACHE.get_or_init(|| {
        MANUAL
            .iter()
            .flat_map(|s| s.pages.iter())
            .map(|p| (p.slug, render_markdown(p.source)))
            .collect()
    })
}

/// "Good tags, and bad ones" → "good-tags-and-bad-ones".
fn slugify(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn render_markdown(source: &str) -> Rendered {
    let options = Options::ENABLE_TABLES | Options::ENABLE_SMART_PUNCTUATION | Options::ENABLE_STRIKETHROUGH;
    let mut events: Vec<Event> = Parser::new_ext(source, options).collect();

    // Give each heading an id from its text, and note the level-2 ones.
    let mut outline = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let level = *level;
            let mut text = String::new();
            let mut j = i + 1;
            while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                if let Event::Text(t) | Event::Code(t) = &events[j] {
                    text.push_str(t);
                }
                j += 1;
            }
            let mut id = slugify(&text);
            let n = seen.entry(id.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                id = format!("{id}-{n}");
            }
            if level == HeadingLevel::H2 {
                outline.push((id.clone(), text.clone()));
            }
            events[i] = Event::Start(Tag::Heading {
                level,
                id: Some(CowStr::from(id)),
                classes: vec![],
                attrs: vec![],
            });
            i = j;
        }
        i += 1;
    }

    let text = events
        .iter()
        .filter_map(|e| match e {
            Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
            Event::SoftBreak | Event::HardBreak => Some(" "),
            Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::Heading(_) | TagEnd::TableCell) => Some(" "),
            _ => None,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());

    // Callouts: a blockquote opening with one of these bold words.
    for (word, class) in [("Example", "example"), ("Tip", "tip"), ("Note", "note"), ("Watch out", "warn")] {
        out = out.replace(
            &format!("<blockquote>\n<p><strong>{word}"),
            &format!("<blockquote class=\"callout {class}\">\n<p><strong>{word}"),
        );
    }
    // Wide tables scroll sideways on a phone instead of stretching the page.
    out = out.replace("<table>", "<div class=\"table-wrap\"><table>").replace("</table>", "</table></div>");

    Rendered { html: out, outline, text }
}

fn find(slug: &str) -> Option<(usize, &'static Section, &'static Page)> {
    let mut n = 0;
    for s in MANUAL {
        for p in s.pages {
            if p.slug == slug {
                return Some((n, s, p));
            }
            n += 1;
        }
    }
    None
}

fn all_pages() -> Vec<(&'static Section, &'static Page)> {
    MANUAL.iter().flat_map(|s| s.pages.iter().map(move |p| (s, p))).collect()
}

/// The contents, for the side navigation and the index.
fn contents(current: Option<&str>) -> Value {
    Value::from_serialize(
        MANUAL
            .iter()
            .map(|s| {
                json!({
                    "title": s.title,
                    "open": current.is_none_or(|c| s.pages.iter().any(|p| p.slug == c)),
                    "pages": s.pages.iter().map(|p| json!({
                        "slug": p.slug, "title": p.title, "summary": p.summary,
                        "current": Some(p.slug) == current,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    )
}

// --- handlers --------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct HelpQuery {
    /// Present for the slide-in help panel.
    panel: Option<String>,
}

async fn index(
    author: AuthenticatedAuthor,
    State(_state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Html<String>> {
    // Everything search needs, in one small JSON block: titles, summaries,
    // section names, and each page's plain text.
    let index: Vec<serde_json::Value> = all_pages()
        .iter()
        .map(|(s, p)| {
            json!({ "slug": p.slug, "title": p.title, "summary": p.summary,
                    "section": s.title, "text": rendered()[p.slug].text })
        })
        .collect();
    let ctx = context! {
        title => "Studio manual",
        contents => contents(None),
        search_data => super::routes::script_json(&json!(index)),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_help_index.html", ctx)
}

async fn page(
    author: AuthenticatedAuthor,
    jar: CookieJar,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(q): Query<HelpQuery>,
) -> AppResult<Html<String>> {
    let (n, section, p) = find(&slug).ok_or(AppError::NotFound("help page"))?;
    let r = &rendered()[p.slug];
    let pages = all_pages();
    let link = |i: usize| pages.get(i).map(|(_, p)| json!({ "slug": p.slug, "title": p.title }));
    let ctx = context! {
        title => p.title,
        slug => p.slug,
        page_title => p.title,
        summary => p.summary,
        section => section.title,
        body => Value::from_safe_string(r.html.clone()),
        outline => Value::from_serialize(r.outline.iter().map(|(id, t)| json!({"id": id, "title": t})).collect::<Vec<_>>()),
        prev => n.checked_sub(1).and_then(link),
        next => link(n + 1),
        contents => contents(Some(p.slug)),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    if q.panel.is_some() {
        render(&environment(), "studio_help_panel.html", ctx)
    } else {
        respond(&environment(), &headers, "studio_help.html", ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_renders_and_slugs_are_unique() {
        let mut slugs = std::collections::HashSet::new();
        for (_, p) in all_pages() {
            assert!(slugs.insert(p.slug), "duplicate slug {}", p.slug);
            let r = render_markdown(p.source);
            assert!(!r.html.is_empty(), "{} is empty", p.slug);
            assert!(!p.source.starts_with("# "), "{} repeats its title as a heading", p.slug);
        }
    }

    /// Every /studio/help/... link in the manual points at a real page and,
    /// if it names a section, a real heading on that page.
    #[test]
    fn internal_links_resolve() {
        for (_, p) in all_pages() {
            let html = &render_markdown(p.source).html;
            for part in html.split("href=\"/studio/help/").skip(1) {
                let target = &part[..part.find('"').unwrap()];
                let (slug, anchor) = target.split_once('#').unwrap_or((target, ""));
                let (_, _, to) = find(slug).unwrap_or_else(|| panic!("{}: link to missing page {slug}", p.slug));
                if !anchor.is_empty() {
                    let to_html = render_markdown(to.source).html;
                    assert!(to_html.contains(&format!("id=\"{anchor}\"")), "{}: no #{anchor} on {slug}", p.slug);
                }
            }
        }
    }

    /// Every help("page", "tip", "anchor") in the Studio's templates opens a
    /// real page, at a real heading.
    #[test]
    fn template_help_buttons_resolve() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/studio/templates");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap();
            for call in src.split("help(\"").skip(1) {
                let args: Vec<&str> = call[..call.find(") }}").unwrap()].split("\", \"").collect();
                let slug = args[0].trim_end_matches('"');
                let (_, _, page) = find(slug).unwrap_or_else(|| panic!("{path:?}: help({slug}) has no page"));
                if let Some(anchor) = args.get(2) {
                    let anchor = anchor.trim_end_matches('"');
                    let html = render_markdown(page.source).html;
                    assert!(html.contains(&format!("id=\"{anchor}\"")), "{path:?}: no #{anchor} on {slug}");
                }
                checked += 1;
            }
        }
        assert!(checked > 20, "only {checked} help buttons found");
    }

    #[test]
    fn headings_get_ids_and_callouts_get_classes() {
        let r = render_markdown("## Good tags, and bad ones\n\n> **Example:** solar\n");
        assert!(r.html.contains("<h2 id=\"good-tags-and-bad-ones\">"));
        assert!(r.html.contains("class=\"callout example\""));
        assert_eq!(r.outline[0].0, "good-tags-and-bad-ones");
    }
}
