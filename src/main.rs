use maud::html;

const SOURCE_CODE: &[u8] = include_repo::include_repo!();

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let mut app = tide::new();


    // index
    app.at("/").get(|_| async move {
        Ok(html! {
            (maud::DOCTYPE)
            h2 { "pkg.golang.fail packages" }

            p.body {
                "Hello!" br;
                "Welcome to this site with some golang packages! Check em out:" br;
                ul {
                    li { a href="/tuple" { "n-ary generic tuple" } }
                    li { a href="/result" { "n-ary generic result" } }
                } br;
                br;
                br;
            }
            h4 { "FAQ" }
            p.faq {
                h5 { "What language is this site in?" }
                p { "Rust, of course" }
                h5 { "Where's the source code?" }
                p {
                    "On github " a href="https://github.com/euank/gupl" { "here" } ", or as an archive " a href="/source.tar.gz" { "here" } "." br;
                }
                h5 { "Should I use any of these packages?" }
                p { a href="https://youtu.be/gIVGftIWt4s?t=2" { "I just don't know" } }
            };
        })
    });


    // source code
    app.at("/source.tar.gz").get(|_| async move {
    });


    app.listen("0.0.0.0:8080").await?;
    Ok(())
}
