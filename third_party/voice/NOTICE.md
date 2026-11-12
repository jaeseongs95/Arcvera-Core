# Native voice libraries in Codex releases

Codex release packages with voice include dynamically linked GStreamer and GLib
libraries and selected plugins, plus their native library dependencies. These
components have their own copyrights and licenses. Their notices accompany
this file in `licenses/`: `LGPL-2.1.txt` for GStreamer and GLib,
`proxy-libintl.txt` for libintl, `libffi.txt`, `PCRE2.md` and `sljit.txt`
for PCRE2, `Opus.txt`, and `zlib.txt`. GVDB is included with GLib under LGPL.

The exact upstream versions, source archive URLs and SHA-256 digests are in
`sources.json` alongside this notice. The corresponding build, runtime
projection and package scripts are in the public Codex source tree under
`third_party/voice/`. The source commit for this package is recorded in
`manifest.json`. The native libraries remain separate dynamic libraries in
platform-specific runtime directories. Replacements must be compatible with
the package and, on macOS, have valid code signatures. Build tools listed in
`sources.json` are build inputs, not bundled runtime libraries.

## Microsoft Visual C++ runtime (Windows)

Arcvera Windows packages do not include Microsoft runtime DLLs or installers.
Native voice requires a separately installed Microsoft Visual C++ v14 runtime.
Users must obtain it directly from Microsoft and maintain it separately.
See the package's `RUNTIME.md` for English and Korean instructions.
The Apache and LGPL licenses for other components do not license Microsoft files.
The source's `windows-crt.json` records historical build provenance only;
it is not a current distribution inventory or permission to redistribute.

Arcvera Windows 패키지에는 Microsoft 런타임 DLL이나 설치기가 포함되지 않습니다.
네이티브 음성에 필요한 Visual C++ v14 런타임은 사용자가 Microsoft에서 직접
취득하여 별도로 설치하고 업데이트해야 합니다. 영문·한글 안내는 패키지의
`RUNTIME.md`를 확인하십시오. 다른 구성요소의 Apache·LGPL 라이선스는 Microsoft
파일에 대한 권리를 부여하지 않습니다. 소스의 `windows-crt.json`은 과거 빌드
출처 기록이며 현재 배포 목록이나 재배포 권한을 뜻하지 않습니다.

Microsoft provides the following license terms and redistribution information:

- [Visual C++ v14 Redistributable and Runtime license terms](https://visualstudio.microsoft.com/license-terms/vs2026-ga-visualcpp-v14-redist-runtime/)
- [Visual C++ Redistributable downloads and support information](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170)
- [Visual Studio 2026 redistribution information](https://learn.microsoft.com/en-us/visualstudio/releases/2026/redistribution)
