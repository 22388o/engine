use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use reqwest::header;
use reqwest::header::{HeaderMap, HeaderValue};
use uuid::Uuid;

// generate the right header for digital ocean with token
pub fn get_header_with_bearer(token: &str) -> HeaderMap<HeaderValue> {
    let mut headers = header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("Authorization", format!("Bearer {}", token).parse().unwrap());
    headers
}

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

pub fn compute_image_tag<P: AsRef<Path> + Hash, T: AsRef<Path> + Hash>(
    root_path: P,
    dockerfile_path: &Option<T>,
    environment_variables: &BTreeMap<String, String>,
    commit_id: &str,
) -> String {
    // Image tag == hash(root_path) + commit_id truncate to 127 char
    // https://github.com/distribution/distribution/blob/6affafd1f030087d88f88841bf66a8abe2bf4d24/reference/regexp.go#L41
    let mut hasher = DefaultHasher::new();

    // If any of those variables changes, we'll get a new hash value, what results in a new image
    // build and avoids using cache. It is important to build a new image, as those variables may
    // affect the build result even if user didn't change his code.
    root_path.hash(&mut hasher);

    if dockerfile_path.is_some() {
        // only use when a Dockerfile is used to prevent build cache miss every single time
        // we redeploy an app with a env var changed with Buildpacks.
        dockerfile_path.hash(&mut hasher);
        environment_variables.hash(&mut hasher);
    }

    let mut tag = format!("{}-{}", hasher.finish(), commit_id);
    tag.truncate(127);

    tag
}

pub fn to_short_id(id: &Uuid) -> String {
    format!("z{}", id.to_string().split_at(8).0)
}

#[cfg(test)]
mod tests_utilities {
    use crate::utilities::compute_image_tag;
    use std::collections::BTreeMap;

    #[test]
    fn test_get_image_tag() {
        let image_tag = compute_image_tag(
            &"/".to_string(),
            &Some("Dockerfile".to_string()),
            &BTreeMap::new(),
            "63d8c437337416a7067d3f358197ac47d003fab9",
        );

        let image_tag_2 = compute_image_tag(
            &"/".to_string(),
            &Some("Dockerfile.qovery".to_string()),
            &BTreeMap::new(),
            "63d8c437337416a7067d3f358197ac47d003fab9",
        );

        assert_ne!(image_tag, image_tag_2);

        let image_tag_3 = compute_image_tag(
            &"/xxx".to_string(),
            &Some("Dockerfile.qovery".to_string()),
            &BTreeMap::new(),
            "63d8c437337416a7067d3f358197ac47d003fab9",
        );

        assert_ne!(image_tag, image_tag_3);

        let image_tag_3_2 = compute_image_tag(
            &"/xxx".to_string(),
            &Some("Dockerfile.qovery".to_string()),
            &BTreeMap::new(),
            "63d8c437337416a7067d3f358197ac47d003fab9",
        );

        assert_eq!(image_tag_3, image_tag_3_2);

        let image_tag_4 = compute_image_tag(
            &"/".to_string(),
            &None as &Option<&str>,
            &BTreeMap::new(),
            "63d8c437337416a7067d3f358197ac47d003fab9",
        );

        let mut env_vars_5 = BTreeMap::new();
        env_vars_5.insert("toto".to_string(), "key".to_string());

        let image_tag_5 = compute_image_tag(
            &"/".to_string(),
            &None as &Option<&str>,
            &env_vars_5,
            "63d8c437337416a7067d3f358197ac47d003fab9",
        );

        assert_eq!(image_tag_4, image_tag_5);
    }
}
