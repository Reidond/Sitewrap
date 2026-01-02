use std::{env, fs, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};

fn main() -> Result<()> {
    compile_resources()?;
    Ok(())
}

fn compile_resources() -> Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let blueprint_dir = manifest_dir.join("data/blueprints");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let ui_out = out_dir.join("ui");
    fs::create_dir_all(&ui_out)?;

    let blueprint_compiler =
        env::var("BLUEPRINT_COMPILER").unwrap_or_else(|_| "blueprint-compiler".to_string());
    let glib_compile_resources =
        env::var("GLIB_COMPILE_RESOURCES").unwrap_or_else(|_| "glib-compile-resources".to_string());

    ensure_tool(
        &blueprint_compiler,
        "Install blueprint-compiler (GNOME SDK) or set BLUEPRINT_COMPILER path",
    )?;
    ensure_tool(
        &glib_compile_resources,
        "Install glib-compile-resources (glib2) or set GLIB_COMPILE_RESOURCES path",
    )?;

    if !blueprint_dir.exists() {
        bail!(
            "Blueprint directory missing: {:?}. Add .blp files under data/blueprints.",
            blueprint_dir
        );
    }

    let mut compiled_files: Vec<String> = Vec::new();

    for entry in fs::read_dir(&blueprint_dir)
        .with_context(|| format!("read blueprint dir {blueprint_dir:?}"))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("blp") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("blueprint stem")?;
        let target = ui_out.join(format!("{stem}.ui"));
        println!("cargo:rerun-if-changed={}", path.display());
        let status = Command::new(&blueprint_compiler)
            .args(["compile", "--output"])
            .arg(&target)
            .arg(&path)
            .status()
            .with_context(|| format!("invoke {blueprint_compiler} on {path:?}"))?;
        if !status.success() {
            bail!("blueprint-compiler failed for {path:?}");
        }
        compiled_files.push(format!("ui/{stem}.ui"));
    }

    if compiled_files.is_empty() {
        bail!("no blueprint (.blp) files found in {:?}", blueprint_dir);
    }

    let xml_path = out_dir.join("resources.gresource.xml");
    let xml = render_gresource_xml(&compiled_files);
    fs::write(&xml_path, xml).context("write generated gresource xml")?;

    let target_path = out_dir.join("sitewrap.gresource");
    let status = Command::new(&glib_compile_resources)
        .arg("--sourcedir")
        .arg(&out_dir)
        .arg("--target")
        .arg(&target_path)
        .arg(&xml_path)
        .status()
        .with_context(|| format!("invoke {glib_compile_resources}"))?;
    if !status.success() {
        bail!("glib-compile-resources failed");
    }

    Ok(())
}

fn ensure_tool(bin: &str, help: &str) -> Result<()> {
    let check = Command::new(bin).arg("--version").output();
    match check {
        Ok(out) if out.status.success() => Ok(()),
        Ok(_) => bail!("{bin} is present but failed --version. {help}"),
        Err(err) => bail!("{bin} not found ({err}). {help}"),
    }
}

fn render_gresource_xml(files: &[String]) -> String {
    let file_entries = files
        .iter()
        .map(|f| format!("    <file compressed=\"true\">{f}</file>"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gresources>
  <gresource prefix="/xyz/andriishafar/sitewrap">
{file_entries}
  </gresource>
</gresources>
"#
    )
}
