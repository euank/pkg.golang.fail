use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::Context;
use axum::{
    body::HttpBody,
    extract::{Path, Query},
    http::StatusCode,
    response::Html,
    response::IntoResponse,
    routing::get,
    Router,
};
use maud::html;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const SOURCE_CODE: &[u8] = include_repo::include_repo!();

#[tokio::main]
async fn main() -> Result<(), axum::http::Error> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "example_static_file_server=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    let mut app = Router::new();

    app = app.route("/", get(|_: ()| async move {
        Html(html! {
            (maud::DOCTYPE)
            h2 { "pkg.golang.fail packages" }

            p.body {
                "Hello!" br;
                "Welcome to this site with some golang packages! Check em out:" br;
                ul {
                    li { a href="/tuple" { "n-ary generic tuple" } }
                }
            }
            h3 { "FAQ" }
            p.faq {
                h4 { "What language is this site written in?" }
                p { "Rust, of course" }
                h4 { "Where's the source code?" }
                p {
                    "On github " a href="https://github.com/euank/pkg.golang.fail" { "here" } ", or as an archive " a href="/source.tar.gz" { "here" } "." br;
                }
                h4 { "Should I use any of these packages?" }
                p { a href="https://youtu.be/gIVGftIWt4s?t=2" { "I just don't know" } }
            };
        }.into_string())
    }));

    // source code
    app = app
        .route("/source.tar.gz", get(source_code))
        .route("/tuple", get(tuple))
        .route("/tuple/:n/tuple", get(tuple_n))
        .route(
            "/tuple/:n/tuple.git/*tree",
            get(tuple_n_git_handler).post(tuple_n_git_handler),
        )
        .layer(TraceLayer::new_for_http());

    axum::Server::bind(&std::net::SocketAddr::from(([0, 0, 0, 0], 8080)))
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}

async fn source_code() -> impl IntoResponse {
    axum::http::Response::builder()
        .header("Content-Type", "application/gzip")
        .status(axum::http::StatusCode::OK)
        .body(axum::body::Full::from(SOURCE_CODE))
        .unwrap()
}

const TUPLE_EXAMPLE: &str = include_str!("./tuple_example.go");

async fn tuple() -> impl IntoResponse {
    Html(html! {
        (maud::DOCTYPE)
        h2 { "n-ary generic tuple" }
        p {
            "This package provides a set of n-ary generic tuple type, for all n! " br;
            "Due to some fundamental limitations, we've ended up splitting it up as one N-ary tuple per unique go package, so you'll need a go.mod entry per n-tuple." br;
            br;
            "What does this look like in practice? Well, let's look at some sample code using this tuple type:"
            pre {
                (TUPLE_EXAMPLE)
            }
        }
        p {
            "Import tuples with the pattern shown above, with " code { "pkg.golang.fail/tuple/$NUM/tuple" } " where " code { "$NUM" } " is the desired arity of the tuple type."
        }
    }.into_string())
}

async fn tuple_n(req: axum::http::Request<axum::body::Body>) -> impl IntoResponse {
    let path = req.uri().path();
    let query = req.uri().query().unwrap_or("");

    if query == "go-get=1" {
        Html(html! {
            (maud::DOCTYPE)
            meta name="go-import" content=(format!("pkg.golang.fail{} git https://pkg.golang.fail{}.git", path, path));
        }.into_string())
    } else {
        Html("<h3>No go get parameter</h3>".into())
    }
}

async fn tuple_n_git_handler(
    p: Path<(u64, String)>,
    q: Query<HashMap<String, String>>,
    req: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    match tuple_n_git(p, q, req).await {
        Ok(r) => r,
        Err(e) => {
            println!("unhandled git error: {}", e);
            axum::http::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Full::new(
                    "internal server error".as_bytes().into(),
                ))
                .unwrap()
        }
    }
}

async fn tuple_n_git(
    Path((n, path)): Path<(u64, String)>,
    Query(query): Query<HashMap<String, String>>,
    mut req: axum::http::Request<axum::body::Body>,
) -> Result<axum::http::Response<axum::body::Full<axum::body::Bytes>>, anyhow::Error> {
    // serve up a git repo
    // total hack, _but_ I can't find a good in-memory git repo option, so serve em up via the
    // filesystem.
    let repo = init_repo(n)?;

    // and now handle the clone if request
    match path.as_str() {
        "/info/refs" => {
            if query.get("service").unwrap_or(&"".to_string()) != &"git-upload-pack".to_string() {
                anyhow::bail!("unsupported service; use a better git client");
            }

            let child = std::process::Command::new("git-upload-pack")
                .arg("--advertise-refs")
                .arg(repo)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();

            let output = child.wait_with_output().unwrap();

            if !output.status.success() {
                anyhow::bail!(
                    "git-upload-pack --advertise-refs did not have success: {}",
                    output.status
                );
            }

            let mut resp_body = Vec::with_capacity(output.stdout.len());

            resp_body.extend_from_slice(b"001e# service=git-upload-pack\n0000");
            resp_body.extend_from_slice(&output.stdout);

            axum::http::Response::builder()
                .header(
                    "Content-Type",
                    "application/x-git-upload-pack-advertisement",
                )
                .body(axum::body::Full::new(resp_body.into()))
                .context("error building response")
        }
        "/git-upload-pack" => {
            // TODO: check Content-type
            let body_data = req.body_mut().data().await.unwrap();
            let mut child = std::process::Command::new("git-upload-pack")
                .arg("--stateless-rpc")
                .arg(repo)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit())
                .spawn()
                .unwrap();

            let mut child_in = child.stdin.take().unwrap();
            child_in.write_all(&body_data.unwrap())?;
            child_in.flush()?;
            std::mem::drop(child_in);

            let mut resp_body = child.stdout.take().unwrap();
            let mut resp = Vec::new();
            resp_body.read_to_end(&mut resp)?;

            let child_res = child.wait()?;

            if !child_res.success() {
                anyhow::bail!("git-upload-pack did not exit with success");
            }

            // done, return response
            axum::http::Response::builder()
                .header("Content-Type", "application/x-git-upload-pack-result")
                .body(axum::body::Full::new(resp.into()))
                .context("error building response")
        }
        p => {
            anyhow::bail!("not supported: {}", p);
        }
    }
}

fn init_repo(n: u64) -> Result<std::path::PathBuf, anyhow::Error> {
    let path: std::path::PathBuf = format!("repos/{}", n).into();
    match std::fs::read_dir(&path) {
        Ok(_) => return Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("not found: {}", n);
            // we should create it
        }
        r => {
            r?;
        }
    };

    std::fs::create_dir_all("repos")?;
    // create it
    let tmp = tempfile::TempDir::new_in("repos")?;
    write_nary_tuple(tmp.path(), n)?;

    // repo created; persist the tempfile into place atomically and away we go
    let tmp_path = tmp.into_path();

    match std::fs::rename(&tmp_path, &path) {
        Ok(_) => Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_dir_all(&tmp_path);
            // race, it's already there, so it's right
            Ok(path)
        }
        Err(e) => Err(e)?,
    }
}

const BSD3_LICENSE: &[u8] = include_bytes!("./BSD3_LICENSE");

fn write_nary_tuple(root: &std::path::Path, n: u64) -> Result<(), anyhow::Error> {
    let mut f = std::fs::File::create(root.join("go.mod"))?;
    f.write_all(
        format!(
            r#"module pkg.golang.fail/tuple/{}/tuple

go 1.18"#,
            n
        )
        .as_bytes(),
    )?;
    std::mem::drop(f);

    let constraints = if n == 0 {
        "".into()
    } else {
        format!(
            "[{} any]",
            (0..n)
                .map(|i| format!("T{}", i))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let tuple_members = if n == 0 {
        "".into()
    } else {
        (0..n)
            .map(|i| format!("T{i} T{i}"))
            .collect::<Vec<_>>()
            .join("\n\t")
    };

    let inst_params = if n == 0 {
        "".into()
    } else {
        format!(
            "[{}]",
            (0..n)
                .map(|i| format!("T{}", i))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let multiple_ret = if n == 0 {
        "".into()
    } else {
        format!(
            "({}) ",
            (0..n)
                .map(|i| format!("T{}", i))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let multiple_ret_body = if n == 0 {
        "".into()
    } else {
        format!(
            "return {}",
            (0..n)
                .map(|i| format!("t.T{}", i))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let new_args = (0..n)
        .map(|i| format!("t{i} T{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let struct_inst_body = (0..n)
        .map(|i| format!("t{}", i))
        .collect::<Vec<_>>()
        .join(", ");

    // and now the impl
    let mut f = std::fs::File::create(root.join("tuple.go"))?;
    f.write_all(
        format!(
            r#"// Package tuple provides an {n}-ary tuple implementation. This is useful for cases
// where multiple values should be grouped together, such as for passing across channels.
// Multiple returns and n-ary tuples may be converted between as well by using the 'New' method to
// go from n-ary return to a Tuple, and using Tuple.Unpack to convert in the other direction.
package tuple

// Tuple implements an {n}-ary generic tuple.
// It may be used
type Tuple{constraints} struct {{
	{tuple_members}
}}

// New constructs a new tuple from the given arguments.
func New{constraints}({new_args}) Tuple{inst_params} {{
	return Tuple{inst_params}{{{struct_inst_body}}}
}}

// Unpack unpacks a Tuple via multiple-return of the contained data.
func (t Tuple{inst_params}) Unpack() {multiple_ret}{{
	{multiple_ret_body}
}}
"#
        )
        .as_bytes(),
    )?;
    std::mem::drop(f);
    {
        std::fs::File::create(root.join("LICENSE"))?.write_all(BSD3_LICENSE)?;
    }

    // all files ready, now try to make a reproducible git repo

    let repo = git2::Repository::init(&root)?;

    let mut tree = repo.index()?;
    tree.add_path(std::path::Path::new("go.mod"))?;
    tree.add_path(std::path::Path::new("tuple.go"))?;
    tree.add_path(std::path::Path::new("LICENSE"))?;
    let tree_id = tree.write_tree()?;
    std::mem::drop(tree);

    let tree = repo.find_tree(tree_id)?;

    let sig = git2::Signature::new(
        "Euan Kemp",
        &format!("{}{}{}", "euank", "@", "euank.com"),
        &git2::Time::new(420, 69),
    )?;

    repo.commit(
        Some("refs/heads/main"),
        &sig,
        &sig,
        "Initial commit",
        &tree,
        &[],
    )?;
    repo.set_head("refs/heads/main")?;
    Ok(())
}
