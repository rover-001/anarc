//! Reader-mode script assembly.

const READABILITY: &str = include_str!("../assets/Readability.js");

/// Builds the script that toggles the active document into a readable article view.
pub fn toggle_script() -> String {
    format!(
        r#"(()=>{{
if(window.__lumenReaderActive){{location.reload();return;}}
try{{
{READABILITY}
const article=new Readability(document.cloneNode(true)).parse();
if(!article||!article.content){{return;}}
const parsed=new DOMParser().parseFromString(article.content,"text/html");
parsed.querySelectorAll("script,iframe,object,embed,form,button,input,textarea,select,link,meta").forEach(node=>node.remove());
parsed.querySelectorAll("*").forEach(node=>{{
for(const attr of [...node.attributes]){{
if(attr.name.toLowerCase().startsWith("on"))node.removeAttribute(attr.name);
if(["href","src","xlink:href"].includes(attr.name.toLowerCase())&&/^\\s*javascript:/i.test(attr.value))node.removeAttribute(attr.name);
}}
}});
const content=document.createElement("main");
content.id="lumen-reader-content";
content.innerHTML=parsed.body.innerHTML;
document.body.replaceChildren(content);
document.title=article.title||document.title;
const style=document.createElement("style");
style.id="lumen-reader-style";
style.textContent=`html,body{{margin:0!important;min-height:100%;background:#15161a!important;color:#e6e7eb!important}}
body{{font:18px/1.6 system-ui,sans-serif!important}}
#lumen-reader-content{{box-sizing:border-box;max-width:720px;margin:0 auto;padding:56px 28px 96px}}
#lumen-reader-content h1,#lumen-reader-content h2,#lumen-reader-content h3{{line-height:1.25;margin:1.6em 0 .6em}}
#lumen-reader-content a{{color:#9aa6ff}} #lumen-reader-content img{{max-width:100%;height:auto}}
#lumen-reader-content pre{{overflow:auto;padding:16px;background:#111216;border-radius:12px}}
#lumen-reader-content blockquote{{margin-left:0;padding-left:18px;border-left:3px solid #282b33;color:#b8bac2}}
#lumen-reader-content table{{max-width:100%;border-collapse:collapse;display:block;overflow:auto}}
#lumen-reader-content th,#lumen-reader-content td{{padding:8px;border:1px solid #282b33}}`;
document.head.append(style);
window.__lumenReaderActive=true;
}}catch(error){{console.error("Lumen reader mode failed",error);}}
}})();"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_script_embeds_readability_and_reader_styling() {
        let script = toggle_script();
        assert!(script.contains("new Readability(document.cloneNode(true)).parse()"));
        assert!(script.contains("location.reload()"));
        assert!(script.contains("#15161a"));
        assert!(script.contains("max-width:720px"));
        assert!(script.contains("document.body.replaceChildren(content)"));
    }
}
