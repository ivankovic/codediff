# Copyright 2026 Marko Ivankovic
# Distributed under the terms of the GNU Affero General Public License v3 or later

EAPI=8

CRATES="
	adler2@2.0.1
	aho-corasick@1.1.5
	allocator-api2@0.2.21
	anes@0.1.6
	anstream@1.0.0
	anstyle-parse@1.0.0
	anstyle-query@1.1.5
	anstyle-wincon@3.0.11
	anstyle@1.0.14
	anyhow@1.0.104
	autocfg@1.5.1
	base64@0.22.1
	bincode@1.3.3
	bit-set@0.8.0
	bit-vec@0.8.0
	bitflags@2.13.1
	bumpalo@3.20.3
	bytes@1.12.1
	cassowary@0.3.0
	cast@0.3.0
	castaway@0.2.4
	cc@1.4.2
	cfg-if@1.0.4
	ciborium-io@0.2.2
	ciborium-ll@0.2.2
	ciborium@0.2.2
	clap@4.6.6
	clap_builder@4.6.6
	clap_complete@4.6.9
	clap_derive@4.6.4
	clap_lex@1.1.0
	clap_mangen@0.2.33
	colorchoice@1.0.5
	compact_str@0.7.1
	confy@2.0.0
	crc32fast@1.5.0
	criterion-plot@0.5.0
	criterion@0.5.1
	crossbeam-channel@0.5.16
	crossbeam-deque@0.8.7
	crossbeam-epoch@0.9.20
	crossbeam-utils@0.8.22
	crossterm@0.27.0
	crossterm_winapi@0.9.1
	crunchy@0.2.4
	csv-core@0.1.13
	csv@1.4.0
	deranged@0.5.8
	displaydoc@0.2.7
	either@1.17.0
	equivalent@1.0.2
	errno@0.3.14
	etcetera@0.10.0
	fallible-iterator@0.3.0
	fallible-streaming-iterator@0.1.9
	fancy-regex@0.16.2
	fastrand@2.5.0
	find-msvc-tools@0.1.10
	flate2@1.1.9
	fnv@1.0.7
	foldhash@0.1.5
	form_urlencoded@1.2.2
	futures-channel@0.3.33
	futures-core@0.3.33
	futures-executor@0.3.33
	futures-io@0.3.33
	futures-macro@0.3.33
	futures-sink@0.3.33
	futures-task@0.3.33
	futures-util@0.3.33
	futures@0.3.33
	getrandom@0.2.17
	getrandom@0.4.3
	git2@0.20.4
	half@2.7.1
	hashbrown@0.15.5
	hashbrown@0.17.1
	hashlink@0.10.0
	heck@0.5.0
	hermit-abi@0.5.2
	home@0.5.11
	icu_collections@2.1.1
	icu_locale_core@2.1.1
	icu_normalizer@2.1.1
	icu_normalizer_data@2.1.1
	icu_properties@2.1.2
	icu_properties_data@2.1.2
	icu_provider@2.1.1
	idna@1.1.0
	idna_adapter@1.2.1
	indexmap@2.14.0
	indoc@2.0.7
	is-terminal@0.4.17
	is_terminal_polyfill@1.70.2
	itertools@0.10.5
	itertools@0.12.1
	itertools@0.13.0
	itoa@1.0.18
	jobserver@0.1.35
	js-sys@0.3.104
	lazy_static@1.5.0
	libc@0.2.189
	libgit2-sys@0.18.7+1.9.6
	libsqlite3-sys@0.35.0
	libssh2-sys@0.3.2
	libz-sys@1.1.29
	linked-hash-map@0.5.6
	linux-raw-sys@0.12.1
	litemap@0.8.2
	lock_api@0.4.14
	log@0.4.33
	lru@0.12.5
	matchers@0.2.0
	memchr@2.8.3
	metrohash@1.0.7
	miniz_oxide@0.8.9
	mio@0.8.11
	mio@1.2.2
	nu-ansi-term@0.50.3
	num-conv@0.2.2
	num-traits@0.2.19
	num_cpus@1.17.0
	once_cell@1.21.4
	once_cell_polyfill@1.70.2
	onig@6.5.3
	onig_sys@69.9.3
	oorandom@11.1.5
	openssl-probe@0.1.6
	openssl-sys@0.9.117
	parking_lot@0.12.5
	parking_lot_core@0.9.12
	paste@1.0.15
	percent-encoding@2.3.2
	pin-project-lite@0.2.17
	pkg-config@0.3.33
	plist@1.8.0
	plotters-backend@0.3.7
	plotters-svg@0.3.7
	plotters@0.3.7
	potential_utf@0.1.5
	powerfmt@0.2.0
	ppv-lite86@0.2.21
	proc-macro2@1.0.107
	quick-xml@0.38.4
	quote@1.0.47
	r-efi@6.0.0
	rand@0.8.7
	rand_chacha@0.3.1
	rand_core@0.6.4
	ratatui@0.26.3
	rayon-core@1.13.0
	rayon@1.12.0
	redox_syscall@0.5.18
	regex-automata@0.4.18
	regex-syntax@0.8.11
	regex@1.13.1
	roff@1.1.1
	rusqlite@0.37.0
	rustc-hash@2.1.3
	rustix@1.1.4
	rustversion@1.0.23
	ryu@1.0.23
	same-file@1.0.6
	scopeguard@1.2.0
	serde@1.0.229
	serde_core@1.0.229
	serde_derive@1.0.229
	serde_json@1.0.151
	serde_spanned@1.1.1
	sharded-slab@0.1.7
	shlex@2.0.1
	signal-hook-mio@0.2.5
	signal-hook-registry@1.4.8
	signal-hook@0.3.18
	simd-adler32@0.3.10
	slab@0.4.12
	smallvec@1.15.2
	socket2@0.6.5
	stability@0.2.1
	stable_deref_trait@1.2.1
	static_assertions@1.1.0
	streaming-iterator@0.1.9
	strsim@0.11.1
	strum@0.26.3
	strum@0.28.0
	strum_macros@0.26.4
	strum_macros@0.28.0
	syn@2.0.119
	syn@3.0.3
	synstructure@0.13.2
	syntect@5.3.0
	tempfile@3.27.0
	thiserror-impl@2.0.20
	thiserror@2.0.20
	thread_local@1.1.10
	time-core@0.1.8
	time-macros@0.2.27
	time@0.3.47
	tinystr@0.8.3
	tinytemplate@1.2.1
	tokio-macros@2.7.2
	tokio@1.53.1
	toml@0.9.12+spec-1.1.0
	toml_datetime@0.7.5+spec-1.1.0
	toml_parser@1.1.3+spec-1.1.0
	toml_writer@1.1.2+spec-1.1.0
	tracing-attributes@0.1.31
	tracing-core@0.1.36
	tracing-error@0.2.1
	tracing-log@0.2.0
	tracing-subscriber@0.3.23
	tracing@0.1.44
	tree-sitter-bash@0.25.1
	tree-sitter-c-sharp@0.23.5
	tree-sitter-c@0.24.2
	tree-sitter-cpp@0.23.4
	tree-sitter-css@0.25.0
	tree-sitter-go@0.25.0
	tree-sitter-html@0.23.2
	tree-sitter-java@0.23.5
	tree-sitter-javascript@0.25.0
	tree-sitter-json@0.24.8
	tree-sitter-kotlin-ng@1.1.0
	tree-sitter-language@0.1.7
	tree-sitter-lua@0.2.0
	tree-sitter-php@0.24.2
	tree-sitter-python@0.25.0
	tree-sitter-r@1.3.0
	tree-sitter-ruby@0.23.1
	tree-sitter-rust@0.24.2
	tree-sitter-scala@0.24.1
	tree-sitter-swift@0.7.3
	tree-sitter-typescript@0.23.2
	tree-sitter-vim@0.4.0
	tree-sitter-xml@0.7.0
	tree-sitter-yaml@0.7.2
	tree-sitter@0.25.10
	two-face@0.5.2+bat-0.26.1
	unicode-ident@1.0.24
	unicode-segmentation@1.13.3
	unicode-truncate@1.1.0
	unicode-width@0.1.14
	url@2.5.8
	utf8_iter@1.0.4
	utf8parse@0.2.2
	valuable@0.1.1
	vcpkg@0.2.15
	walkdir@2.5.0
	wasi@0.11.1+wasi-snapshot-preview1
	wasm-bindgen-macro-support@0.2.127
	wasm-bindgen-macro@0.2.127
	wasm-bindgen-shared@0.2.127
	wasm-bindgen@0.2.127
	web-sys@0.3.104
	winapi-i686-pc-windows-gnu@0.4.0
	winapi-util@0.1.11
	winapi-x86_64-pc-windows-gnu@0.4.0
	winapi@0.3.9
	windows-link@0.2.1
	windows-sys@0.48.0
	windows-sys@0.59.0
	windows-sys@0.61.2
	windows-targets@0.48.5
	windows-targets@0.52.6
	windows_aarch64_gnullvm@0.48.5
	windows_aarch64_gnullvm@0.52.6
	windows_aarch64_msvc@0.48.5
	windows_aarch64_msvc@0.52.6
	windows_i686_gnu@0.48.5
	windows_i686_gnu@0.52.6
	windows_i686_gnullvm@0.52.6
	windows_i686_msvc@0.48.5
	windows_i686_msvc@0.52.6
	windows_x86_64_gnu@0.48.5
	windows_x86_64_gnu@0.52.6
	windows_x86_64_gnullvm@0.48.5
	windows_x86_64_gnullvm@0.52.6
	windows_x86_64_msvc@0.48.5
	windows_x86_64_msvc@0.52.6
	winnow@0.7.15
	winnow@1.0.4
	writeable@0.6.3
	yaml-rust@0.4.5
	yoke-derive@0.8.2
	yoke@0.8.3
	zerocopy-derive@0.8.56
	zerocopy@0.8.56
	zerofrom-derive@0.1.7
	zerofrom@0.1.8
	zerotrie@0.2.4
	zerovec-derive@0.11.3
	zerovec@0.11.6
	zmij@1.0.23
"

RUST_MIN_VER="1.85.0"

# bash-completion-r1 for `newbashcomp` in src_install; without it that call is an unbound command
# and the install phase dies.
inherit bash-completion-r1 cargo

DESCRIPTION="Fast, robust, syntax-aware code diffing using tree-sitter ASTs"
HOMEPAGE="https://github.com/ivankovic/codediff"
SRC_URI="
	https://github.com/ivankovic/codediff/archive/refs/tags/v${PV}.tar.gz -> ${P}.tar.gz
	${CARGO_CRATE_URIS}
"

# The ebuild's own license is AGPL-3+; the trailing list covers the 293 vendored crates, whose
# licenses cargo.eclass expects to be enumerated here. Regenerate with `pycargoebuild` if the
# dependency set changes - the list below was read off the crates in Cargo.lock and is the usual
# Rust-ecosystem spread.
LICENSE="AGPL-3+"
LICENSE+=" Apache-2.0 BSD BSD-2 ISC MIT MPL-2.0 Unicode-DFS-2016 Unlicense ZLIB"
SLOT="0"
KEYWORDS="~amd64 ~arm64"

# Every tree-sitter grammar is compiled from C at build time, so a C compiler is required. It is
# part of @system on Gentoo, hence no explicit DEPEND - but it is why this package is not a pure
# Rust build and why the build takes noticeably longer than the crate count alone suggests.

# Built with the default `tui` feature only - the product binary. The `stats` and `test-fixtures`
# features gate dataset-analysis dev tools that need git2 (OpenSSL, libssh2) and a bundled SQLite;
# they are not part of the shipped product, so there is no USE flag for them.

# `lto = "fat"` plus `codegen-units = 1` in the release profile (see Cargo.toml) makes this a slow
# single-threaded link. That is deliberate upstream - codediff is CPU-bound at run time - but it is
# worth knowing before reporting the build as hung.

src_test() {
	# The GitHub tag tarball carries tests/ and the fixture corpus (the crates.io tarball excludes
	# both), so the suite is runnable here - but the corpus tests are the accuracy benchmark, which
	# needs the `test-fixtures` feature and a large amount of time and memory. Restrict to the
	# library's own unit tests, which are what a packaging sanity check actually wants.
	cargo_src_test --lib
}

src_install() {
	cargo_src_install

	# Generated from the same clap definition as --help, by the binary that was just built. Native
	# build only; if this package ever grows a cross-compile path, these have to move to a
	# host-built artifact instead.
	# Hardcoded rather than $(usex debug ...): that idiom needs a `debug` USE flag in IUSE, and
	# this package deliberately offers no debug build - the release profile's lto/codegen-units
	# settings are the point of it (see Cargo.toml).
	local codediff="${S}/target/release/codediff"
	"${codediff}" util man > "${T}/${PN}.1" || die "failed to generate man page"
	doman "${T}/${PN}.1"

	"${codediff}" util completions bash > "${T}/${PN}.bash" || die
	newbashcomp "${T}/${PN}.bash" "${PN}"

	"${codediff}" util completions zsh > "${T}/_${PN}" || die
	insinto /usr/share/zsh/site-functions
	doins "${T}/_${PN}"

	"${codediff}" util completions fish > "${T}/${PN}.fish" || die
	insinto /usr/share/fish/vendor_completions.d
	doins "${T}/${PN}.fish"

	dodoc README.md CONTRIBUTING.md
}

pkg_postinst() {
	elog "Configure codediff as git's diff tool with:  codediff git configure"
	elog "Configure codediff as jj's diff tool with:   codediff jj configure"
}
