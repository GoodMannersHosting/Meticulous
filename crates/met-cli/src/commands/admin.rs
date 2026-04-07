//! Admin-only API commands (requires an admin user / token).

use crate::OutputFormat;
use crate::api_client::{ApiClient, Result};
use crate::output::{
    build_table, format_status, format_timestamp, print_serialized, print_table, status_icon,
};
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JobQueueEntry {
    pub job_run_id: Option<String>,
    pub run_id: String,
    pub job_id: Option<String>,
    pub job_name: String,
    pub job_status: String,
    pub attempt: i32,
    pub job_run_created_at: String,
    pub run_number: i64,
    pub run_status: String,
    pub pipeline_id: String,
    pub pipeline_name: String,
    pub project_id: String,
    pub project_slug: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobQueueListResponse {
    pub count: usize,
    pub data: Vec<JobQueueEntry>,
}

pub async fn job_queue(client: &ApiClient, limit: u32, format: OutputFormat) -> Result<()> {
    #[derive(Serialize)]
    struct Query {
        limit: u32,
    }

    let limit = limit.min(500);
    let response: JobQueueListResponse = client
        .get_with_query("/admin/ops/job-queue", &Query { limit })
        .await?;

    match format {
        OutputFormat::Table => {
            if response.data.is_empty() {
                println!("No pending or queued jobs.");
                return Ok(());
            }
            let mut table =
                build_table(&["Job status", "Job", "Project", "Pipeline", "Run", "Since"]);
            for row in &response.data {
                let started = format_timestamp(&row.job_run_created_at);
                table.add_row(vec![
                    Cell::new(format!(
                        "{} {}",
                        status_icon(&row.job_status),
                        format_status(&row.job_status)
                    )),
                    Cell::new(&row.job_name),
                    Cell::new(&row.project_slug),
                    Cell::new(&row.pipeline_name),
                    Cell::new(format!(
                        "#{} ({})",
                        row.run_number,
                        &row.run_id[..8.min(row.run_id.len())]
                    )),
                    Cell::new(started),
                ]);
            }
            print_table(&table);
            println!("\n{} job run(s) waiting for an agent", response.count);
        }
        _ => print_serialized(&response, format)?,
    }
    Ok(())
}
