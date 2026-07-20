//! Shared serde helpers.

/// Distinguishes an absent JSON field from one explicitly set to `null`.
///
/// Paired with `#[serde(default)]` on an `Option<Option<T>>`: a missing field
/// yields `None`, while a field present as `null` yields `Some(None)`. That is
/// what lets a PATCH body mean "leave this alone" with `{}` and "clear this"
/// with `{"field": null}`.
///
/// Without it, serde deserializes `null` straight to `None` — identical to
/// absent — and a nullable field becomes impossible to clear once set. The
/// failure is quiet: the request succeeds and the value simply does not
/// change.
pub fn present<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Patch {
        #[serde(default, deserialize_with = "super::present")]
        caption: Option<Option<String>>,
    }

    #[test]
    fn absent_field_is_none() {
        let p: Patch = serde_json::from_str("{}").unwrap();
        assert_eq!(p.caption, None, "absent means leave alone");
    }

    #[test]
    fn explicit_null_is_some_none() {
        let p: Patch = serde_json::from_str(r#"{"caption": null}"#).unwrap();
        assert_eq!(p.caption, Some(None), "explicit null means clear");
    }

    #[test]
    fn value_is_some_some() {
        let p: Patch = serde_json::from_str(r#"{"caption": "hi"}"#).unwrap();
        assert_eq!(p.caption, Some(Some("hi".to_string())));
    }
}
