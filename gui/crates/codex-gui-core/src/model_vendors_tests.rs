//! Unit coverage of the vendor-catalog merge rules.
#![allow(clippy::expect_used)]

use super::ConfiguredProvider;
use super::VendorCatalog;

fn provider(id: &str, name: Option<&str>, base_url: &str) -> ConfiguredProvider {
    ConfiguredProvider {
        id: id.to_string(),
        name: name.map(str::to_string),
        base_url: base_url.to_string(),
    }
}

#[test]
fn skeleton_lists_every_builtin_vendor_with_suggestions() {
    let catalog = VendorCatalog::skeleton();
    let ids: Vec<&str> = catalog.groups().iter().map(|g| g.id.as_str()).collect();
    assert!(ids.contains(&"openai"));
    assert!(ids.contains(&"glm"));
    assert!(ids.contains(&"kimi"));
    assert!(catalog.groups().iter().all(|g| !g.configured));

    let glm = catalog
        .groups()
        .iter()
        .find(|g| g.id == "glm")
        .expect("glm");
    // Current GLM tier in entry order; duplicate plan suggestions collapse.
    let slugs: Vec<&str> = glm.models.iter().map(|model| model.slug.as_str()).collect();
    assert_eq!(slugs, ["glm-5.3", "glm-5.3-flash", "glm-5.3-flashx"]);
}

#[test]
fn configured_provider_marks_its_preset_group() {
    let mut catalog = VendorCatalog::skeleton();
    catalog.apply_config(
        Some("glm-5.3"),
        Some("glm"),
        &[provider(
            "glm",
            Some("GLM (智谱)"),
            "https://open.bigmodel.cn/api/v1",
        )],
    );

    let glm = catalog
        .groups()
        .iter()
        .find(|g| g.id == "glm")
        .expect("glm");
    assert!(glm.configured);
    assert_eq!(glm.provider_id.as_deref(), Some("glm"));
    assert_eq!(catalog.current(), (Some("glm"), Some("glm-5.3")));
}

#[test]
fn base_url_matching_works_even_when_the_key_differs() {
    let mut catalog = VendorCatalog::skeleton();
    catalog.apply_config(
        None,
        None,
        &[provider("my-glm", None, "https://open.bigmodel.cn/api/v1/")],
    );

    let glm = catalog
        .groups()
        .iter()
        .find(|g| g.id == "glm")
        .expect("glm");
    assert!(glm.configured);
    assert_eq!(glm.provider_id.as_deref(), Some("my-glm"));
    // No extra custom group was appended for the matched provider.
    assert_eq!(
        catalog.groups().len(),
        VendorCatalog::skeleton().groups().len()
    );
}

#[test]
fn unknown_providers_join_as_custom_groups_and_stay_idempotent() {
    let mut catalog = VendorCatalog::skeleton();
    let baseline = catalog.groups().len();
    let gateway = provider("mygw", Some("My Gateway"), "https://gw.example.com/v1");
    catalog.apply_config(None, None, std::slice::from_ref(&gateway));
    catalog.apply_config(None, None, std::slice::from_ref(&gateway));

    assert_eq!(catalog.groups().len(), baseline + 1);
    let custom = catalog.groups().last().expect("custom group");
    assert_eq!(custom.name, "My Gateway");
    assert!(custom.configured);
    // The matched-preset groups were not duplicated either.
    assert_eq!(catalog.groups().iter().filter(|g| g.id == "glm").count(), 1);
}

#[test]
fn catalog_entries_replace_the_active_providers_suggestions() {
    let mut catalog = VendorCatalog::skeleton();
    catalog.apply_config(
        Some("glm-5-turbo"),
        Some("glm"),
        &[provider("glm", None, "https://open.bigmodel.cn/api/v1")],
    );
    catalog.attach_catalog(&[
        (
            "glm-5.3".to_string(),
            "glm-5.3".to_string(),
            "Z.ai flagship".to_string(),
        ),
        (
            "glm-5-turbo".to_string(),
            "glm-5-turbo".to_string(),
            "Agent-optimized".to_string(),
        ),
    ]);

    let glm = catalog
        .groups()
        .iter()
        .find(|g| g.id == "glm")
        .expect("glm");
    assert_eq!(glm.models.len(), 2);
    assert_eq!(glm.models[1].description, "Agent-optimized");
    // Untouched vendors keep their suggestions.
    let kimi = catalog
        .groups()
        .iter()
        .find(|g| g.id == "kimi")
        .expect("kimi");
    assert!(!kimi.models.is_empty());
}

#[test]
fn empty_catalog_keeps_the_suggestion_list() {
    let mut catalog = VendorCatalog::skeleton();
    catalog.apply_config(
        None,
        Some("glm"),
        &[provider("glm", None, "https://open.bigmodel.cn/api/v1")],
    );
    catalog.attach_catalog(&[]);
    let glm = catalog
        .groups()
        .iter()
        .find(|g| g.id == "glm")
        .expect("glm");
    assert_eq!(glm.models.len(), 3);
}

#[test]
fn catalog_entries_from_another_vendor_stay_out_of_the_active_provider_group() {
    let mut catalog = VendorCatalog::skeleton();
    catalog.apply_config(
        Some("deepseek-v4-pro"),
        Some("deepseek"),
        &[provider("deepseek", None, "https://api.deepseek.com")],
    );
    // A static GLM catalog must not overwrite the DeepSeek group's
    // suggestions; the rows land in the group whose slugs they match.
    catalog.attach_catalog(&[
        ("glm-5.3".to_string(), "glm-5.3".to_string(), String::new()),
        (
            "glm-5-turbo".to_string(),
            "glm-5-turbo".to_string(),
            String::new(),
        ),
    ]);

    let deepseek = catalog
        .groups()
        .iter()
        .find(|g| g.id == "deepseek")
        .expect("deepseek");
    let slugs: Vec<&str> = deepseek.models.iter().map(|m| m.slug.as_str()).collect();
    assert_eq!(slugs, ["deepseek-flash", "deepseek-v4-pro"]);

    let glm = catalog
        .groups()
        .iter()
        .find(|g| g.id == "glm")
        .expect("glm");
    let slugs: Vec<&str> = glm.models.iter().map(|m| m.slug.as_str()).collect();
    assert_eq!(slugs, ["glm-5.3", "glm-5-turbo"]);
}

#[test]
fn catalog_entries_fill_the_active_groups_empty_suggestions() {
    let mut catalog = VendorCatalog::skeleton();
    catalog.apply_config(
        Some("gw-model"),
        Some("mygw"),
        &[provider("mygw", None, "https://gw.example.com/v1")],
    );
    // The custom gateway has no suggestions of its own, so the live
    // catalog is authoritative for it.
    catalog.attach_catalog(&[(
        "gw-model".to_string(),
        "gw-model".to_string(),
        String::new(),
    )]);

    let gateway = catalog.groups().last().expect("custom group");
    let slugs: Vec<&str> = gateway.models.iter().map(|m| m.slug.as_str()).collect();
    assert_eq!(slugs, ["gw-model"]);
}

#[test]
fn unmatched_catalog_rows_fall_back_to_the_active_provider_group() {
    let mut catalog = VendorCatalog::skeleton();
    catalog.apply_config(
        Some("deepseek-flash"),
        Some("deepseek"),
        &[provider("deepseek", None, "https://api.deepseek.com")],
    );
    // Rows matching no group still describe the active provider; the
    // fallback keeps them visible instead of dropping them.
    catalog.attach_catalog(&[
        (
            "deepseek-next".to_string(),
            "deepseek-next".to_string(),
            String::new(),
        ),
        (
            "deepseek-mini".to_string(),
            "deepseek-mini".to_string(),
            String::new(),
        ),
    ]);

    let deepseek = catalog
        .groups()
        .iter()
        .find(|g| g.id == "deepseek")
        .expect("deepseek");
    let slugs: Vec<&str> = deepseek.models.iter().map(|m| m.slug.as_str()).collect();
    assert_eq!(slugs, ["deepseek-next", "deepseek-mini"]);
}
