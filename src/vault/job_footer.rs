//! Optional vault footer append when linked jobs succeed (Phase V1).

use chrono::{DateTime, Utc};

use crate::load_product_config;
use crate::vault::store::vault_store;
use crate::workspace::store::workspace_store;

const FOOTER_MARKER_PREFIX: &str = "<!-- medousa:job:";

pub fn maybe_append_job_success_footers(
    job_id: &str,
    job_title: &str,
    finished_at: DateTime<Utc>,
) -> usize {
    let config = load_product_config();
    if !config.vault.job_success_footer_enabled {
        return 0;
    }

    let associations = workspace_store().associations(job_id);
    if associations.vault_paths.is_empty() {
        return 0;
    }

    let marker = format!("{FOOTER_MARKER_PREFIX}{job_id}:succeeded -->");
    let footer = format!(
        "\n\n---\n{marker}\n- **Job completed:** {job_title}\n- **Finished:** {finished_at}\n"
    );

    let mut appended = 0usize;
    for path in &associations.vault_paths {
        let Ok(existing) = vault_store().read_content(path) else {
            continue;
        };
        if existing.contains(&marker) {
            continue;
        }
        let updated = format!("{existing}{footer}");
        if vault_store()
            .write_content(path, &updated, None)
            .is_ok()
        {
            appended += 1;
        }
    }
    appended
}
