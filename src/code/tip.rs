/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

//! Path-based file classification into the four gross `Type` categories.
//!
//! Only the file name and extension are consulted, never the contents, so it is cheap enough to
//! decide which files of a multi-million-file corpus to read at all. The tables list what occurs in
//! the corpus in volume, not everything. Ambiguous extensions (`.in`, `.t`, `.raw`, `.mod`, `.res`,
//! `.rc`, `.def`) stay unclassified on purpose: a wrong category is worse than none, since the
//! statistics read exactly the files classified as Code or Configuration.

use crate::code::Type;
use crate::code::language::language_for_extension;

/// Returns the type from path. If possible, the subtype is added as the string value to the enums
/// gross Code/Config/Data categorization.
pub fn type_from_path(path: &std::path::Path) -> Option<Type> {
    if let Some(filename) = path.file_name()
        && let Some(t) = type_from_filename(&filename.to_string_lossy())
    {
        return Some(t);
    }

    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if let Some(t) = type_from_extension(&ext) {
        return Some(t);
    }

    None
}

/// Returns the type from the extension.
///
/// If possible, the subtype is added as the string value to the enums gross Code/Config/Data categorization.
pub fn type_from_extension(ext: &str) -> Option<Type> {
    if language_for_extension(ext).is_some() {
        return Some(Type::Code(String::from("Uncategorized")));
    }

    // web-platform-tests ships `foo.js^headers^` sidecars next to fixtures: plain-text HTTP headers,
    // whatever the extension in front of the marker says.
    if ext.ends_with("^headers^") {
        return Some(Type::Data(String::from("Text")));
    }

    let code = |subtype: &str| Some(Type::Code(String::from(subtype)));
    let configuration = |subtype: &str| Some(Type::Configuration(String::from(subtype)));
    let data = |subtype: &str| Some(Type::Data(String::from(subtype)));
    let documentation = |subtype: &str| Some(Type::Documentation(String::from(subtype)));

    match ext {
        // ── Code ────────────────────────────────────────────────────────────────────────────────
        // Languages with no tree-sitter grammar compiled in. Still code: read and counted, not
        // parsed.
        "adb" | "ads" | "ada" => code("Ada"),
        "s" | "asm" | "nasm" | "inc" | "inl" | "lds" | "ld" => code("Assembly"),
        "m" | "mm" | "cu" | "cuh" | "hip" | "ispc" | "cl" | "cocci" | "i" | "ipp" => {
            code("Uncategorized")
        }
        "ml" | "mli" | "mll" | "mly" | "mlw" | "re" | "sml" | "fs" | "fsi" | "fsx" => code("ML"),
        "lisp" | "lsp" | "scm" | "ss" | "sld" | "sch" | "clj" | "cljs" | "cljc" | "edn" | "rkt"
        | "fnl" | "elpi" => code("Lisp"),
        "f" | "f77" | "f90" | "f95" | "f03" | "f08" | "for" | "ftn" | "fpp" => code("Fortran"),
        "v" | "sv" | "svh" | "vhd" | "vhdl" | "bsv" | "ys" | "lus" | "sdc" | "xdc" => {
            code("Hardware")
        }
        "dts" | "dtsi" | "dtso" => code("DeviceTree"),
        "frag" | "vert" | "comp" | "geom" | "tesc" | "tese" | "glsl" | "hlsl" | "wgsl"
        | "metal" | "sksl" | "osl" | "fx" | "cg" | "vs" => code("Shader"),
        "tcl" | "ps1" | "psm1" | "bat" | "cmd" | "awk" | "sed" | "fish" | "zsh" | "ksh" | "csh"
        | "nsh" | "nsi" | "vbs" | "ahk" | "bats" | "vader" | "hurl" | "bsh" | "rake" | "do" => {
            code("Script")
        }
        "pl" | "pm" | "pod6" | "raku" => code("Perl"),
        "phpt" | "jsp" | "erb" | "haml" | "slim" | "vue" | "svelte" | "astro" | "less" | "sass"
        | "coffee" | "elm" | "purs" | "mjml" | "pcss" | "es" => code("Web"),
        "hx" | "cr" | "d" | "erl" | "hrl" | "ex" | "exs" | "hs" | "lhs" | "jl" | "nim" | "zig"
        | "vala" | "vapi" | "odin" | "pony" | "c3" | "vb" | "bas" | "gd" | "as" | "groovy"
        | "gvy" | "kts" | "sc" | "pas" | "pp" | "dpr" | "cob" | "cbl" | "nix" | "moon" | "wren"
        | "ly" | "csd" | "cwl" | "aug" | "te" | "gf" | "rbs" | "rbi" | "pyx" | "pxd" | "pxi"
        | "qml" | "mir" | "ll" | "mlir" | "td" | "smt2" | "lean" | "dfy" | "acl2" | "idr"
        | "agda" | "tla" | "mzn" | "dzn" | "asl" | "bi" | "aj" | "icn" | "rg" | "gi" | "why"
        | "cvc" | "bpl" | "lkt" | "c3t" | "fth" | "qc" | "pov" | "scad" | "ddl" | "pg" | "des"
        | "feature" => code("Uncategorized"),
        "ipynb" | "rmd" | "qmd" => code("Notebook"),
        "y" | "l" | "lex" | "yacc" | "flex" | "jison" | "g" | "g4" | "peg" | "pegjs" | "ebnf"
        | "abnf" | "bnf" | "lalrpop" | "pest" | "rl" | "langium" => code("Grammar"),
        "webidl" | "ipdl" | "ipdlh" | "thrift" | "fbs" | "capnp" | "graphql" | "gql" | "avsc"
        | "smithy" | "ice" | "aidl" | "wit" => code("Interface"),
        "xsd" | "dtd" | "rng" | "rnc" | "yang" | "asn1" | "asn" | "jsonschema" => code("Schema"),
        "tmpl" | "tpl" | "template" | "j2" | "jinja" | "jinja2" | "hbs" | "handlebars"
        | "mustache" | "ejs" | "twig" | "ftl" | "vm" | "gotmpl" | "xsl" | "xslt" | "ptl"
        | "py-tpl" => code("Template"),
        "mk" | "mak" | "am" | "ac" | "m4" | "cmake" | "build" | "bzl" | "gradle" | "sbt"
        | "mill" | "gyp" | "gypi" | "gn" | "gni" | "pro" | "pri" | "qrc" | "vcxproj" | "vcproj"
        | "csproj" | "fsproj" | "vbproj" | "wixproj" | "filters" | "props" | "targets" | "sln"
        | "wxs" | "wxl" | "gpr" | "hxml" | "ebuild" | "eclass" | "mozbuild" | "dsp" | "dsw"
        | "jam" | "rockspec" | "gemspec" | "podspec" | "nuspec" | "cabal" | "opam" | "spec"
        | "bnd" | "recipe" | "dockerfile" => code("Build"),
        "ui" | "glade" | "xib" | "storyboard" | "xaml" | "blp" => code("UI"),
        "dot" | "gv" | "puml" | "plantuml" | "mmd" | "d2" => code("Diagram"),
        "wdl" | "nf" | "smk" => code("Workflow"),

        // ── Configuration ───────────────────────────────────────────────────────────────────────
        "ini" | "cfg" | "toml" | "conf" | "cnf" | "config" | "properties" | "prefs" | "reg"
        | "inf" | "desktop" | "service" | "socket" | "timer" | "rules" | "plist" | "xcconfig"
        | "pbxproj" | "xcscheme" | "xcworkspacedata" | "entitlements" | "xcprivacy" | "ruleset"
        | "cue" | "hcl" | "tf" | "tfvars" | "dhall" | "env" | "jnlp" | "dvc" | "wrap"
        | "defconfig" | "modulemap" | "pc" | "editorconfig" | "kdl" | "xcu" | "xcs" | "theme"
        | "network" | "link" | "fc" | "tablet" | "fs-uae-controller" | "install" | "gemfile" => {
            configuration("")
        }
        "resw" | "resx" | "xtb" | "xlb" | "ulf" | "lng" | "strings" | "stringsdict" | "pot" => {
            configuration("Localization")
        }
        "lock" | "sum" => configuration("Lock"),
        "manifest" | "webmanifest" | "appxmanifest" | "control" | "mf" => configuration("Manifest"),
        "gitattributes" | "gitignore" | "gitmodules" | "dockerignore" | "npmignore"
        | "eslintignore" | "prettierignore" | "vscodeignore" | "cvsignore" | "hgignore"
        | "nvmrc" | "npmrc" | "bazelrc" | "flake8" | "coveragerc" | "pylintrc" | "luacheckrc"
        | "eslintrc" | "prettierrc" | "babelrc" | "jshintrc" | "stylelintrc" | "mocharc"
        | "swcrc" | "yarnrc" | "clang-format" | "clang-tidy" | "ocamlformat" | "rspec"
        | "htaccess" | "code-snippets" | "code-workspace" | "sublime-project"
        | "sublime-settings" | "tidy_config" => configuration("DevEx"),

        // ── Data ────────────────────────────────────────────────────────────────────────────────
        "txt" | "csv" | "rst" | "txtpb" | "pbtxt" | "stderr" | "doc" | "diff" | "tsv" | "tab"
        | "log" | "lst" | "list" | "text" | "nfo" | "patch" | "orig" | "rej" | "bak" | "txtar"
        | "ndjson" | "jsonl" | "jsonld" | "jsonc" | "json5" | "geojson" | "topojson" | "ics"
        | "vcf" | "eml" | "mbox" | "srt" | "vtt" | "sub" | "ass" | "lrc" | "ttl" | "n3" | "nt"
        | "nq" | "trig" | "rdf" | "owl" | "rq" | "har" | "pnach" | "dic" | "aff" | "sug"
        | "hyf" | "headers" | "sdf" | "fasta" | "fa" | "fastq" | "fq" | "gff" | "gtf" | "bed"
        | "sam" | "gb" | "embl" | "cif" | "xyz" | "mol" | "gpx" | "kml" | "ldif" | "textproto"
        | "data" | "sgml" | "urdf" | "mtl" | "obj" | "stl" | "off" | "ply" | "dxf" | "rast"
        | "tbl" | "sym" | "map" | "idx" | "coverage" | "cov" | "cov-map" | "gcov" | "lcov"
        | "tfstate" | "example" | "sample" | "test" | "tests" | "tst" | "qtest" | "ctst"
        | "fqtest" | "litmus" | "benchmark" | "input" | "request" | "response" | "certspec"
        | "keyspec" | "pkcs7spec" | "caddyfiletest" | "explain" | "explain_shape" | "dump"
        | "dmp" | "trace" | "mscx" | "qxf" | "pcc" | "pmf" | "pho" | "hrc" | "facts" | "skin"
        | "spr" | "sprite" | "grp" | "dtstyle" | "ucm" | "utf8" => data("Text"),
        "out" | "output" | "outerr" | "err" | "expected" | "expected_out" | "expected_err"
        | "expect" | "expect-noinput" | "golden" | "gold" | "snap" | "snapshot" | "ambr"
        | "baseline" | "fixed" | "good" | "wrong" | "result" | "results" | "stdout" | "preview"
        | "exp" | "oracle" | "reference" | "detok" => data("Expectation"),
        "gz" | "zip" | "tar" | "bz2" | "xz" | "lz4" | "zst" | "br" | "7z" | "rar" | "tgz"
        | "tbz2" | "txz" | "cab" | "cpio" | "squashfs" | "jar" | "war" | "ear" | "apk" | "ipa"
        | "deb" | "rpm" | "msi" | "dmg" | "iso" | "img" | "whl" | "egg" | "nupkg" | "gem"
        | "crate" | "xpi" | "oxt" | "vsix" | "asar" | "mar" | "z" | "hqx" | "par2" | "nzb"
        | "mscz" => data("Archive"),
        "svg" | "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "ico"
        | "icns" | "cur" | "ani" | "xpm" | "xbm" | "pbm" | "pgm" | "ppm" | "pnm" | "avif"
        | "heic" | "heif" | "jxl" | "psd" | "xcf" | "kra" | "ora" | "tga" | "dds" | "exr"
        | "hdr" | "emf" | "wmf" | "eps" | "ai" | "jp2" | "apng" | "svgz" | "pcx" | "dib"
        | "hlc" | "glcd" => data("Image"),
        "mp3" | "waw" | "wav" | "flac" | "acc" | "aac" | "ogg" | "m4a" | "opus" | "aiff"
        | "aif" | "wma" | "mid" | "midi" | "xm" | "it" | "s3m" | "wv" | "ape" | "ac3" | "sf2"
        | "sfz" => data("Audio"),
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "m4v" | "m4s" | "mpg" | "mpeg" | "ogv" | "flv"
        | "wmv" | "3gp" | "vob" | "swf" | "mpd" | "m3u8" => data("Video"),
        "ttf" | "otf" | "woff" | "woff2" | "eot" | "ttc" | "pfb" | "pfa" | "pfm" | "afm"
        | "fea" | "sfd" | "bdf" | "pcf" | "fnt" | "glif" | "glyph" | "flf" | "ttx"
        | "designspace" | "fontinfo" => data("Font"),
        "sqlite" | "sqlite3" | "db" | "db3" | "mdb" | "accdb" | "parquet" | "feather" | "arrow"
        | "orc" | "dbf" | "rdb" | "sav" | "dta" | "sas7bdat" | "fdb" | "mmdb" | "odb" => {
            data("Database")
        }
        "bin" | "dat" | "exe" | "dll" | "class" | "so" | "o" | "wasm" | "symbols" | "idl"
        | "types" | "po" | "mo" | "qm" | "pyc" | "pyo" | "a" | "lib" | "dylib" | "elf" | "dex"
        | "hex" | "bc" | "pdb" | "syso" | "ko" | "bcmap" | "pb" | "onnx" | "pt" | "pth"
        | "safetensors" | "npy" | "npz" | "h5" | "hdf5" | "pkl" | "pickle" | "joblib"
        | "tflite" | "traineddata" | "blend" | "fbx" | "glb" | "gltf" | "3ds" | "dae" | "g3d"
        | "md3" | "lmp" | "tplg" | "rrd" | "pcap" | "pcapng" | "cap" | "com" | "sys" | "drv"
        | "efi" | "rom" | "fw" | "dtb" | "cdb" | "gob" | "mpack" | "msgpack" | "cbor" | "bson"
        | "avro" | "sst" | "ldb" | "icc" | "icm" | "chm" | "hlp" | "pch" | "gch" | "ilk"
        | "nib" | "fla" | "fz" | "xiz" | "fur" | "fui" | "med" | "pkt" | "dwarfs" => data("Binary"),
        "pdf" | "docx" | "docm" | "dotx" | "odt" | "ods" | "odp" | "odg" | "fodt" | "fods"
        | "fodp" | "fodg" | "ott" | "ots" | "otp" | "xlsx" | "xls" | "xlsm" | "xlsb" | "pptx"
        | "ppt" | "rtf" | "epub" | "sxw" | "sxc" | "sxi" | "pages" | "numbers" | "key" | "wpd"
        | "djvu" | "xps" | "ps" | "dvi" | "lwp" => data("Document"),
        "pem" | "crt" | "cer" | "der" | "csr" | "p12" | "pfx" | "p7b" | "p7s" | "p7c" | "pub"
        | "gpg" | "pgp" | "asc" | "sig" | "crl" | "jks" | "keystore" | "cert" | "cat"
        | "pkcs12" | "kdbx" => data("Security"),
        "md5" | "sha" | "sha1" | "sha256" | "sha512" | "chksum" | "sums" => data("Checksum"),

        // ── Documentation ───────────────────────────────────────────────────────────────────────
        "mdx" | "mkd" | "mdown" | "mkdn" | "adoc" | "asciidoc" | "org" | "tex" | "ltx" | "sty"
        | "cls" | "bib" | "bst" | "texi" | "texinfo" | "info" | "dox" | "docbook" | "dita"
        | "ditamap" | "pod" | "man" | "wiki" | "mediawiki" | "apib" | "rdoc" | "ronn" | "scd"
        | "schelp" | "readme" | "changelog" | "qhp" | "typ" | "wakka" | "page" => {
            documentation("General")
        }
        "license" | "licence" => documentation("Legal"),

        _ => {
            // Man pages: a section digit, optionally followed by a subsection tag (`3pm`, `1ssl`,
            // `3x`). Checked after the explicit tables so `7z` above stays an archive.
            let mut chars = ext.chars();
            match chars.next() {
                Some('1'..='9') if chars.all(|c| c.is_ascii_lowercase()) => documentation("Manual"),
                _ => None,
            }
        }
    }
}

/// Returns the type from the file name.
///
/// If possible, the subtype is added as the string value to the enums gross Code/Config/Data categorization.
pub fn type_from_filename(filename: &str) -> Option<Type> {
    let code = |subtype: &str| Some(Type::Code(String::from(subtype)));
    let configuration = |subtype: &str| Some(Type::Configuration(String::from(subtype)));
    let data = |subtype: &str| Some(Type::Data(String::from(subtype)));
    let documentation = |subtype: &str| Some(Type::Documentation(String::from(subtype)));

    let exact = match filename {
        "BUILD" | "Makefile" | "makefile" | "GNUmakefile" | "Dockerfile" | "Containerfile"
        | "CMakeLists.txt" | "meson.build" | "meson_options.txt" | "meson.options"
        | "SConstruct" | "SConscript" | "SCsub" | "Kconfig" | "Kbuild" | "Justfile"
        | "justfile" | "Rakefile" | "Podfile" | "Vagrantfile" | "Jenkinsfile" | "Earthfile"
        | "Brewfile" | "Fastfile" | "Appfile" | "Tiltfile" | "Snakefile" | "WORKSPACE"
        | "WORKSPACE.bazel" | "MODULE.bazel" | "BUCK" | "dune" | "dune-project"
        | "dune-workspace" | "configure" | "install-sh" | "depcomp" | "missing" | "compile"
        | "config.guess" | "config.sub" | "mkfile" | "gradlew" | "rules" | "setup" | "console" => {
            code("Build")
        }
        "OWNERS"
        | "CODEOWNERS"
        | ".gitignore"
        | ".gitattributes"
        | ".gitmodules"
        | ".mailmap"
        | ".git-blame-ignore-revs"
        | ".dockerignore"
        | ".npmignore"
        | ".npmrc"
        | ".nvmrc"
        | ".prettierrc"
        | ".prettierignore"
        | ".eslintrc"
        | ".eslintignore"
        | ".babelrc"
        | ".vscodeignore"
        | ".cvsignore"
        | ".hgignore"
        | ".hgtags"
        | ".bzrignore"
        | ".keep"
        | ".gitkeep"
        | ".htaccess"
        | ".env"
        | ".bazelrc"
        | ".bazelversion"
        | ".bazelignore"
        | ".project"
        | ".classpath"
        | ".cproject"
        | ".kunitconfig"
        | ".import-restrictions"
        | ".yarnrc"
        | ".tool-versions"
        | ".ruby-version"
        | ".python-version"
        | ".node-version"
        | ".terraform-version"
        | ".luacheckrc"
        | ".flake8"
        | ".coveragerc"
        | ".pylintrc"
        | ".rspec"
        | ".ocamlformat"
        | ".clang-format"
        | ".clang-tidy"
        | ".editorconfig"
        | ".gcloudignore"
        | ".helmignore"
        | ".browserslistrc"
        | ".watchmanconfig" => configuration("DevEx"),
        "Doxyfile" | "config" | "aliases" | "useradd" | "compat" | "control" | "copyright"
        | "LINGUAS" | "POTFILES" | "POTFILES.in" | "Gemfile" | "Pipfile" | "Procfile"
        | "requirements.txt" | "constraints.txt" | "MANIFEST.in" | "go.mod" | "properties"
        | "Manifest" | "format" | "watch" | "docs" | "dirs" | "links" | "install" => {
            configuration("")
        }
        "LICENSE" | "LICENCE" | "COPYING" | "COPYRIGHT" | "NOTICE" | "PATENTS" | "AUTHORS"
        | "CONTRIBUTORS" | "MAINTAINERS" | "CREDITS" | "THANKS" | "SECURITY_CONTACTS" => {
            documentation("Legal")
        }
        "README" | "README.md" | "INSTALL" | "HACKING" | "TODO" | "NEWS" | "CHANGES"
        | "ChangeLog" | "CHANGELOG" | "changelog" | "HISTORY" => documentation("General"),
        "VERSION" | "version" | "mimetype" | "passwd" | "group" | "shadow" | "gshadow" | "HEAD"
        | "request" | "response" | "input" | "chksum" | "cmd" | "mlr" | "status" => data("Text"),
        "experr" | "expout" | "expected" | "output" | "out" => data("Expectation"),
        _ => None,
    };
    if exact.is_some() {
        return exact;
    }

    // Families that are usually suffixed: `Dockerfile.dev`, `Makefile.am`, `LICENSE-MIT`,
    // `COPYING.LIB`, `README.rst`, `CHANGELOG.old`. Exact names above win, so `README.md` keeps
    // its subtype and `Makefile.in`'s `.in` extension never gets a say.
    let upper = filename.to_ascii_uppercase();
    if filename.starts_with("Dockerfile.")
        || filename.starts_with("Containerfile.")
        || filename.starts_with("Makefile.")
        || filename.starts_with("makefile.")
        || filename.starts_with("GNUmakefile.")
    {
        return code("Build");
    }
    if upper.starts_with("LICENSE") || upper.starts_with("LICENCE") || upper.starts_with("COPYING")
    {
        return documentation("Legal");
    }
    if upper.starts_with("README") || upper.starts_with("CHANGELOG") {
        return documentation("General");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn type_from_extension_for_invalid_extensions() {
        assert!(type_from_extension("invalid_extension_for_tests").is_none());
    }

    #[test]
    fn type_from_extension_for_valid_extensions() {
        assert!(matches!(
            type_from_extension("ini"),
            Some(Type::Configuration(_))
        ));
        assert!(matches!(type_from_extension("svg"), Some(Type::Data(_))));
    }

    #[test]
    fn grammar_backed_extensions_are_code_before_any_table() {
        assert_eq!(
            type_from_extension("rs"),
            Some(Type::Code("Uncategorized".into()))
        );
        assert_eq!(
            type_from_extension("jsx"),
            Some(Type::Code("Uncategorized".into()))
        );
    }

    #[test]
    fn ambiguous_extensions_stay_unclassified() {
        // `.in` is an autoconf template of anything (`Makefile.in`, `config.h.in`) as often as
        // it is a test input; `.t` is a Perl test as often as a template. Neither gets a guess.
        for ext in ["in", "t", "raw", "mod", "0"] {
            assert!(type_from_extension(ext).is_none(), "{ext}");
        }
    }

    #[test]
    fn man_page_sections() {
        assert_eq!(
            type_from_extension("1"),
            Some(Type::Documentation("Manual".into()))
        );
        assert_eq!(
            type_from_extension("3pm"),
            Some(Type::Documentation("Manual".into()))
        );
        // Archives keep their explicit entry even though `7z` starts with a digit.
        assert_eq!(
            type_from_extension("7z"),
            Some(Type::Data("Archive".into()))
        );
        // A digit followed by anything that is not a subsection tag is not a man page.
        assert!(type_from_extension("3D").is_none());
    }

    #[test]
    fn web_platform_test_header_sidecars_are_text() {
        assert_eq!(
            type_from_path(Path::new("wpt/fetch/video.mp4^headers^")),
            Some(Type::Data("Text".into()))
        );
    }

    #[test]
    fn filename_wins_over_extension() {
        assert_eq!(
            type_from_path(Path::new("src/CMakeLists.txt")),
            Some(Type::Code("Build".into()))
        );
        assert_eq!(
            type_from_path(Path::new("Makefile.in")),
            Some(Type::Code("Build".into()))
        );
        assert_eq!(
            type_from_path(Path::new("README.md")),
            Some(Type::Documentation("General".into()))
        );
        assert_eq!(
            type_from_path(Path::new("requirements.txt")),
            Some(Type::Configuration("".into()))
        );
    }

    #[test]
    fn filename_families_by_prefix() {
        assert_eq!(
            type_from_path(Path::new("LICENSE-MIT")),
            Some(Type::Documentation("Legal".into()))
        );
        assert_eq!(
            type_from_path(Path::new("COPYING.LIB")),
            Some(Type::Documentation("Legal".into()))
        );
        assert_eq!(
            type_from_path(Path::new("docs/README.rst")),
            Some(Type::Documentation("General".into()))
        );
        assert_eq!(
            type_from_path(Path::new("Dockerfile.dev")),
            Some(Type::Code("Build".into()))
        );
    }

    #[test]
    fn dotfiles_without_an_extension() {
        // `Path::extension` is `None` for `.gitattributes`, so only the name table can catch it.
        assert_eq!(
            type_from_path(Path::new("/repo/.gitattributes")),
            Some(Type::Configuration("DevEx".into()))
        );
        assert!(type_from_path(Path::new("/repo/.unknownrc")).is_none());
    }

    #[test]
    fn extension_matching_is_case_insensitive() {
        assert_eq!(
            type_from_path(Path::new("boot/start.S")),
            Some(Type::Code("Assembly".into()))
        );
        assert_eq!(
            type_from_path(Path::new("cert/CA.PEM")),
            Some(Type::Data("Security".into()))
        );
    }
}
