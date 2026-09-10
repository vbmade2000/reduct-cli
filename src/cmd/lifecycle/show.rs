// Copyright 2026 ReductStore
// This Source Code Form is subject to the terms of the Mozilla Public
//    License, v. 2.0. If a copy of the MPL was not distributed with this
//    file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::cmd::lifecycle::helpers::format_mode_with_icon;
use crate::cmd::table::{build_info_table, labeled_cell};
use crate::cmd::RESOURCE_PATH_HELP;
use crate::io::reduct::build_client;
use crate::io::std::output;
use crate::parse::Resource;
use clap::{Arg, Command};

pub(super) fn show_lifecycle_cmd() -> Command {
    Command::new("show")
        .about("Show details about a lifecycle policy")
        .arg(
            Arg::new("LIFECYCLE_PATH")
                .help(RESOURCE_PATH_HELP)
                .value_parser(crate::parse::ResourcePathParser::new())
                .required(true),
        )
}

pub(super) async fn show_lifecycle_handler(
    ctx: &crate::context::CliContext,
    args: &clap::ArgMatches,
) -> anyhow::Result<()> {
    let (alias_or_url, lifecycle_name) = args
        .get_one::<Resource>("LIFECYCLE_PATH")
        .unwrap()
        .clone()
        .pair()?;
    let client = build_client(ctx, &alias_or_url).await?;

    let lifecycle = client.get_lifecycle(&lifecycle_name).await?;

    if ctx.json() {
        output!(ctx, "{}", serde_json::to_string(&lifecycle)?);
        return Ok(());
    }

    let mut info_cells = vec![
        labeled_cell("Name", lifecycle.info.name.clone()),
        labeled_cell(
            "Status",
            if lifecycle.info.is_running {
                "✅ Running"
            } else {
                "⏸ Idle"
            },
        ),
        labeled_cell("Mode", format_mode_with_icon(lifecycle.info.mode)),
        labeled_cell(
            "Provisioned",
            if lifecycle.info.is_provisioned {
                "✓"
            } else {
                "-"
            },
        ),
        labeled_cell("Type", format!("{:?}", lifecycle.settings.lifecycle_type)),
        labeled_cell("Bucket", lifecycle.settings.bucket.clone()),
        labeled_cell("Older Than", lifecycle.settings.older_than.clone()),
        labeled_cell("Interval", lifecycle.settings.interval.clone()),
        labeled_cell(
            "Processing Interval",
            lifecycle
                .settings
                .processing_interval
                .clone()
                .unwrap_or_else(|| "None".to_string()),
        ),
        labeled_cell("Entries", format!("{:?}", lifecycle.settings.entries)),
    ];

    let when_value = lifecycle
        .settings
        .when
        .as_ref()
        .map(|value| serde_json::to_string_pretty(value))
        .transpose()?;
    let when_cell_value = when_value
        .map(|value| value.replace('\n', "\n  "))
        .unwrap_or_else(|| "None".to_string());
    info_cells.push(labeled_cell("When", when_cell_value));

    let info_table = build_info_table(info_cells);
    output!(ctx, "{}", info_table);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::lifecycle::tests::{prepare_lifecycle, unique_name};
    use crate::context::tests::{context, MockOutput};
    use crate::context::{CliContext, ContextBuilder};
    use rstest::rstest;

    #[rstest]
    #[tokio::test]
    async fn test_show_lifecycle(context: CliContext) {
        let lifecycle = unique_name("test-lifecycle");
        let bucket = unique_name("test-bucket");

        prepare_lifecycle(&context, &lifecycle, &bucket)
            .await
            .unwrap();

        let args = show_lifecycle_cmd()
            .get_matches_from(vec!["show", format!("local/{}", lifecycle).as_str()]);

        assert_eq!(show_lifecycle_handler(&context, &args).await.unwrap(), ());
        let output = context.stdout().history();
        assert_eq!(output.len(), 1);
        assert!(output[0].contains(&lifecycle));
        assert!(output[0].contains(&bucket));
        assert!(output[0].contains("Mode: ▶ Enabled"));
        assert!(output[0].contains("Type: Delete"));
        assert!(output[0].contains("Older Than: 1h"));
        assert!(output[0].contains("Interval: 10m"));
        assert!(output[0].contains("Processing Interval: None"));
        assert!(output[0].contains("When: None"));
    }

    #[rstest]
    #[tokio::test]
    async fn test_show_lifecycle_with_processing_interval(context: CliContext) {
        let lifecycle = unique_name("test-lifecycle");
        let bucket = unique_name("test-bucket");
        let client = prepare_lifecycle(&context, &lifecycle, &bucket)
            .await
            .unwrap();

        let mut settings = client.get_lifecycle(&lifecycle).await.unwrap().settings;
        settings.processing_interval = Some("1d".to_string());
        client.update_lifecycle(&lifecycle, settings).await.unwrap();

        let args = show_lifecycle_cmd()
            .get_matches_from(vec!["show", format!("local/{}", lifecycle).as_str()]);
        show_lifecycle_handler(&context, &args).await.unwrap();

        let output = context.stdout().history();
        assert_eq!(output.len(), 1);
        assert!(output[0].contains("Processing Interval: 1d"));
    }

    #[rstest]
    #[tokio::test]
    async fn test_show_lifecycle_invalid_path() {
        let args = show_lifecycle_cmd().try_get_matches_from(vec!["show", "local"]);

        assert_eq!(
            args.err().unwrap().to_string(),
            "error: invalid value 'local' for '<LIFECYCLE_PATH>'\n\nFor more information, try '--help'.\n"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_show_lifecycle_json(context: CliContext) {
        let lifecycle = unique_name("test-lifecycle");
        let bucket = unique_name("test-bucket");

        let ctx = ContextBuilder::new()
            .config_path(context.config_path())
            .json(Some(true))
            .output(Box::new(MockOutput::new()))
            .build();

        prepare_lifecycle(&ctx, &lifecycle, &bucket).await.unwrap();

        let args = show_lifecycle_cmd()
            .get_matches_from(vec!["show", format!("local/{}", lifecycle).as_str()]);
        show_lifecycle_handler(&ctx, &args).await.unwrap();

        let output = &ctx.stdout().history()[0];
        let lifecycle_json: serde_json::Value = serde_json::from_str(output).unwrap();

        assert_eq!(lifecycle_json["info"]["name"], serde_json::json!(lifecycle));
        assert_eq!(
            lifecycle_json["info"]["is_provisioned"],
            serde_json::json!(false)
        );
        assert_eq!(
            lifecycle_json["info"]["is_running"],
            serde_json::json!(true)
        );
        assert_eq!(lifecycle_json["info"]["type"], serde_json::json!("delete"));
        assert_eq!(lifecycle_json["info"]["mode"], serde_json::json!("enabled"));
        assert_eq!(lifecycle_json["info"]["last_run"], serde_json::Value::Null);

        assert_eq!(
            lifecycle_json["settings"]["type"],
            serde_json::json!("delete")
        );
        assert_eq!(
            lifecycle_json["settings"]["bucket"],
            serde_json::json!(bucket)
        );
        assert_eq!(lifecycle_json["settings"]["entries"], serde_json::json!([]));
        assert_eq!(
            lifecycle_json["settings"]["older_than"],
            serde_json::json!("1h")
        );
        assert_eq!(
            lifecycle_json["settings"]["interval"],
            serde_json::json!("10m")
        );
        assert_eq!(lifecycle_json["settings"]["when"], serde_json::Value::Null);
        assert_eq!(
            lifecycle_json["settings"]["processing_interval"],
            serde_json::Value::Null
        );
        assert_eq!(
            lifecycle_json["settings"]["mode"],
            serde_json::json!("enabled")
        );
    }
}
