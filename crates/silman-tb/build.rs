fn main() {
    println!("cargo:rerun-if-changed=vendor/fathom");

    // tbprobe.c #includes tbchess.c internally, so only tbprobe.c is compiled.
    // Fathom is plain C (upstream builds with -std=gnu99, plus C11 <stdatomic.h>
    // when threads are enabled); no exotic flags are needed on macOS or Linux.
    cc::Build::new()
        .file("vendor/fathom/tbprobe.c")
        .include("vendor/fathom")
        .flag_if_supported("-std=gnu11")
        .warnings(false)
        .compile("fathom");

    // Fathom's POSIX code path uses pthread mutexes. pthreads live in libSystem
    // on macOS; link explicitly elsewhere on unix (needed for glibc < 2.34 and
    // the BSDs).
    let family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if family == "unix" && os != "macos" {
        println!("cargo:rustc-link-lib=pthread");
    }
}
