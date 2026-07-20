//! Turning uploaded originals into what the app actually displays.
//!
//! Scope is v1 per docs/DESIGN.md §2.8: images and PDFs. Audio and video are
//! rejected at upload time rather than reaching here.

use std::io::Cursor;
use std::process::Stdio;

use anyhow::{bail, Context};
use image::imageops::FilterType;
use image::DynamicImage;

/// Matches the resize ceiling the legacy Python pipeline used, so existing
/// content does not change appearance in the rewrite.
pub const MAX_DIMENSION: u32 = 2048;

/// WebP quality. 85 is the usual point where artefacts stop being visible on
/// photographic content while the file stays far smaller than JPEG.
pub const WEBP_QUALITY: f32 = 85.0;

#[derive(Debug)]
pub struct Processed {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub extension: &'static str,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// Decodes, downscales, and re-encodes an image to WebP.
///
/// `mime_type` selects the decoder rather than sniffing, but a mismatch is not
/// trusted: the decoders themselves reject bytes that are not what they claim.
pub fn process_image(bytes: &[u8], mime_type: &str) -> anyhow::Result<Processed> {
    let decoded = match mime_type {
        // HEIC/HEIF is the iPhone default, and `image` cannot read it. Rather
        // than link libheif, shell out to the decoder that ships with it --
        // one less native dependency compiled into this binary, at the cost of
        // requiring the tool in the runtime image.
        "image/heic" | "image/heif" => decode_heif(bytes)?,
        _ => image::load_from_memory(bytes)
            .context("could not decode image (is it really the type it claims?)")?,
    };

    let resized = downscale(decoded);
    let (width, height) = (resized.width(), resized.height());
    let bytes = encode_webp(&resized)?;

    Ok(Processed {
        bytes,
        mime_type: "image/webp",
        extension: "webp",
        width: Some(width as i32),
        height: Some(height as i32),
    })
}

/// Renders page 1 of a PDF to a WebP thumbnail.
///
/// The PDF itself is served as uploaded (DESIGN.md §2.5: "store as-is"); this
/// output exists only so a document has something to show in a gallery.
pub fn process_pdf(bytes: &[u8]) -> anyhow::Result<Processed> {
    let rendered = pdf_first_page_png(bytes)?;

    let decoded = image::load_from_memory(&rendered)
        .context("could not decode the page rendered from the PDF")?;

    let resized = downscale(decoded);
    let (width, height) = (resized.width(), resized.height());
    let bytes = encode_webp(&resized)?;

    Ok(Processed {
        bytes,
        mime_type: "image/webp",
        extension: "webp",
        width: Some(width as i32),
        height: Some(height as i32),
    })
}

/// Only downscales. Enlarging a small image would inflate the file for no
/// added detail.
fn downscale(image: DynamicImage) -> DynamicImage {
    if image.width() <= MAX_DIMENSION && image.height() <= MAX_DIMENSION {
        return image;
    }

    // Preserves aspect ratio and fits inside the box.
    image.resize(MAX_DIMENSION, MAX_DIMENSION, FilterType::Lanczos3)
}

fn encode_webp(image: &DynamicImage) -> anyhow::Result<Vec<u8>> {
    // WebP has no alpha-free fast path worth branching on here; RGBA8 is
    // accepted by the encoder for both cases.
    let rgba = image.to_rgba8();

    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let encoded = encoder.encode(WEBP_QUALITY);

    Ok(encoded.to_vec())
}

/// Decodes HEIC/HEIF via `heif-convert`, which ships with libheif.
///
/// Uses temp files rather than stdin/stdout. `heif-convert`'s `-` support is
/// recent (libheif 1.16+) and absent from the version in Debian bookworm,
/// where `heif-convert - -` fails with "Unknown file type in -" because it
/// infers the output format from the filename extension and `-` has none.
/// Temp files with real extensions work across every version, and the input
/// `.heic` extension is what tells the tool how to read it.
fn decode_heif(bytes: &[u8]) -> anyhow::Result<DynamicImage> {
    let work = TempWork::new("heif")?;
    let input = work.path("in.heic");
    let output = work.path("out.png");

    std::fs::write(&input, bytes).context("writing HEIC input for heif-convert")?;

    run_to_completion(
        "heif-convert",
        &[
            "-q",
            "100", // the PNG intermediate is lossless; nothing is lost before the WebP encode
            input.to_str().context("temp path is not valid UTF-8")?,
            output.to_str().context("temp path is not valid UTF-8")?,
        ],
    )
    .context("heif-convert failed; is libheif (with an HEVC decoder) installed in this image?")?;

    // heif-convert appends a page suffix when a file holds multiple images, so
    // the exact output name is not guaranteed. Take whichever PNG it wrote.
    let png = work
        .find_output("png")?
        .context("heif-convert produced no PNG output")?;

    image::load_from_memory(&png).context("heif-convert output could not be decoded")
}

/// A self-cleaning temp directory, so a failure partway through does not leave
/// decoded originals lying in the container's filesystem.
struct TempWork {
    dir: std::path::PathBuf,
}

impl TempWork {
    fn new(tag: &str) -> anyhow::Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        // pid + a monotonic counter is unique enough within one process, and
        // avoids a randomness source (which the workflow constraints forbid in
        // some contexts anyway).
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);

        let dir = std::env::temp_dir().join(format!(
            "pintrail-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).context("creating temp work directory")?;

        Ok(Self { dir })
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.dir.join(name)
    }

    fn find_output(&self, extension: &str) -> anyhow::Result<Option<Vec<u8>>> {
        for entry in std::fs::read_dir(&self.dir).context("reading temp work directory")? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                return Ok(Some(std::fs::read(&path)?));
            }
        }
        Ok(None)
    }
}

impl Drop for TempWork {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Runs a subprocess with fixed arguments (no shell) that reads and writes
/// files rather than pipes.
fn run_to_completion(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("could not start {program}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{program} exited with {}: {}",
            output.status,
            stderr.trim().chars().take(300).collect::<String>()
        );
    }

    Ok(())
}

/// Renders the first page of a PDF with poppler's `pdftoppm`.
fn pdf_first_page_png(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    run_with_stdin(
        "pdftoppm",
        &[
            "-png", // PNG rather than JPEG: no generation loss before the WebP encode
            "-f", "1", "-l", "1", // first page only
            "-r", "150", // enough resolution to downscale from cleanly
            "-singlefile",
        ],
        bytes,
    )
    .context("pdftoppm failed; is poppler installed in this image?")
}

/// Runs a subprocess, feeding it `input` on stdin and collecting stdout.
///
/// Both tools are invoked without a shell and with fixed arguments, so no
/// part of an uploaded file can influence the command line.
fn run_with_stdin(program: &str, args: &[&str], input: &[u8]) -> anyhow::Result<Vec<u8>> {
    use std::io::Write;

    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not start {program}"))?;

    // The write happens on this thread while the child is running; both tools
    // stream, so a large input cannot deadlock on a full pipe buffer the way
    // it would if we wrote everything before reading.
    let mut stdin = child.stdin.take().context("child stdin unavailable")?;
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));

    let output = child
        .wait_with_output()
        .with_context(|| format!("{program} did not complete"))?;

    writer
        .join()
        .map_err(|_| anyhow::anyhow!("stdin writer panicked"))?
        .with_context(|| format!("failed writing to {program} stdin"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{program} exited with {}: {}",
            output.status,
            stderr.trim().chars().take(300).collect::<String>()
        );
    }

    if output.stdout.is_empty() {
        bail!("{program} produced no output");
    }

    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_png(width: u32, height: u32) -> Vec<u8> {
        let img = DynamicImage::new_rgb8(width, height);
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn encodes_to_webp_and_reports_dimensions() {
        let out = process_image(&test_png(100, 50), "image/png").unwrap();
        assert_eq!(out.mime_type, "image/webp");
        assert_eq!(out.width, Some(100));
        assert_eq!(out.height, Some(50));
        assert!(!out.bytes.is_empty());
        // WebP files start with RIFF....WEBP
        assert_eq!(&out.bytes[0..4], b"RIFF");
        assert_eq!(&out.bytes[8..12], b"WEBP");
    }

    #[test]
    fn downscales_oversized_images_preserving_aspect_ratio() {
        let out = process_image(&test_png(4096, 2048), "image/png").unwrap();
        assert_eq!(out.width, Some(2048), "long edge clamped to MAX_DIMENSION");
        assert_eq!(out.height, Some(1024), "aspect ratio preserved");
    }

    #[test]
    fn leaves_small_images_at_their_own_size() {
        let out = process_image(&test_png(320, 240), "image/png").unwrap();
        assert_eq!((out.width, out.height), (Some(320), Some(240)));
    }

    #[test]
    fn rejects_bytes_that_are_not_the_declared_type() {
        let err = process_image(b"this is not an image at all", "image/png").unwrap_err();
        assert!(err.to_string().contains("decode"), "got {err}");
    }

    #[test]
    fn subprocess_failure_is_reported_not_swallowed() {
        let err = run_with_stdin("pdftoppm", &["--nonsense-flag"], b"x").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("pdftoppm"), "got {msg}");
    }
}
