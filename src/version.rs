/// Returns a human-readable version string that includes the package
/// version, Git commit hash, build date, and target triple.
#[must_use]
pub fn version_string() -> String {
  let version = env!("CARGO_PKG_VERSION");
  let git_hash = env!("SHORTENER_GIT_HASH");
  let build_date = env!("SHORTENER_BUILD_DATE");
  let target = env!("SHORTENER_TARGET");
  format!("{version} ({git_hash} {build_date} {target})")
}
