/// tauri-build's default Windows manifest (Common Controls v6, needed for
/// its dialog APIs — see `tauri-build`'s own
/// `src/windows-app-manifest.xml`), extended with `longPathAware`. Setting
/// `app_manifest()` fully *replaces* the default rather than merging with
/// it, so the Common Controls dependency is repeated here rather than
/// dropped.
///
/// The `longPathAware` element and its namespace
/// (`http://schemas.microsoft.com/SMI/2016/WindowsSettings`) are copied
/// verbatim from Microsoft's own docs, not guessed — an easy detail to get
/// wrong silently, since an unrecognized namespace/element is just
/// ignored rather than a build error:
/// https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
///
/// This alone isn't sufficient for long paths to work everywhere — it
/// also requires the machine-wide `LongPathsEnabled` registry value,
/// which Nodekeeper can't set for the user. `nk_core::paths::to_verbatim`
/// is the mechanism that works unconditionally, for Nodekeeper's own file
/// I/O; this manifest entry is the belt to that belt-and-suspenders pair
/// (per docs/SPEC.md Phase 1: "application manifest longPathAware plus
/// `\\?\`-prefixed paths where needed").
const WINDOWS_APP_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings xmlns:ws2="http://schemas.microsoft.com/SMI/2016/WindowsSettings">
      <ws2:longPathAware>true</ws2:longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    let windows = tauri_build::WindowsAttributes::new().app_manifest(WINDOWS_APP_MANIFEST);
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
