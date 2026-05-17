pub fn version_string() -> String {
  let version = env!("CARGO_PKG_VERSION");
  let git_hash = env!("SHORTEN_GIT_HASH");
  let build_date = env!("SHORTEN_BUILD_DATE");
  let target = env!("SHORTEN_TARGET");
  format!("{version} ({git_hash} {build_date} {target})")
}
