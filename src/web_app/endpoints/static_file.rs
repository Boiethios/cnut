use crate::web_app::AppState;
use axum::{
    extract::{Path as AxumPath, State as AxumState},
    http::StatusCode,
    response::Html,
};
use std::{ffi::OsStr, path::PathBuf};
use tokio::fs;

pub async fn static_file(
    AxumState(state): AxumState<AppState>,
    AxumPath(path): AxumPath<PathBuf>,
) -> Result<Html<String>, (StatusCode, &'static str)> {
    let path = state.network.temp_directory().join(path);

    let content = fs::read_to_string(&path)
        .await
        .map_err(|_e| (StatusCode::NOT_FOUND, "404: Not Found"))?;

    let content = match path.extension().and_then(OsStr::to_str) {
        Some("toml") => beautify_toml(content),
        Some(_) | None => content,
    };

    Ok(Html(style(content)))
}

fn beautify_toml(input: String) -> String {
    let mut buf = String::from("<code><pre>");

    for line in input.lines() {
        if line.starts_with('[') && line.ends_with(']') {
            buf.push_str(&format!("<span class=\"strong\">{line}</span>"));
        } else {
            buf.push_str(line);
        }
        buf.push('\n');
    }

    buf.push_str("</pre></code>");

    buf
}

fn style(content: String) -> String {
    format!(
        r#"<html lang="en">

<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Casper Utilities for Network Testing</title>
    <style>
        
pre,
code {{
    .strong {{
        font-weight: bold;
        color: green;
    }}
}}
    </style>
</head>

<body>
    {content}
</body>

</html>"#
    )
}
