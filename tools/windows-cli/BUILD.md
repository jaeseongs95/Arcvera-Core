# Windows source build / Windows 소스 빌드

## English

The package's `source/arcvera-core-source.zip` contains the complete committed project tree, including Cargo manifests/lockfile, vendored project code, native-source references, build scripts, packaging tools and licensing notices. `SOURCE.md` records the exact executable source commit. The release verification appendix records the actual compiler, dependency inputs, profile, executable hashes and unverified items. Dependency/native corresponding-source completeness and licensing must be checked before release; a project-only archive does not establish those checks.

Use an isolated Windows x86-64 build directory. Install the MSVC x64 C/C++ build tools and Windows SDK, Rust `1.99.1-x86_64-pc-windows-msvc`, Python 3.11 or later, Git, CMake and the native tools required by the locked dependencies. The upstream `rust-toolchain.toml` remains a historical input; explicitly select the release compiler below. Preserve `Cargo.lock`. Do not copy authentication, user profiles or operational logs into the source/build directory. Consult `third_party/voice/sources.json` and the matching notices when acquiring separately built native helpers.

From the extracted `codex-rs` directory, in a Developer PowerShell with the x64 MSVC environment, the following profile reproduces the release settings. The actual final release appendix is authoritative if it records a later verified override. Resource guards and caches are local build controls; never change machine-wide memory settings.

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CMAKE_BUILD_PARALLEL_LEVEL = '1'
$env:CARGO_PROFILE_RELEASE_OPT_LEVEL = '3'
$env:CARGO_PROFILE_RELEASE_LTO = 'off'
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '2'
$env:CARGO_PROFILE_RELEASE_DEBUG = '0'
$env:CARGO_PROFILE_RELEASE_INCREMENTAL = 'false'
$env:CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_CODEGEN_UNITS = '2'
$env:CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG = '0'
$env:CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_OPT_LEVEL = '0'
$rustRoot = rustc +1.99.1 --print sysroot
$lld = Join-Path $rustRoot 'lib/rustlib/x86_64-pc-windows-msvc/bin/gcc-ld/lld-link.exe'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $lld
$env:RUSTFLAGS = '-C link-arg=/STACK:8388608 -C target-feature=+crt-static -C link-arg=/threads:2'
cargo +1.99.1 fetch --locked --target x86_64-pc-windows-msvc
cargo +1.99.1 build --locked --release --target x86_64-pc-windows-msvc -j1 --config 'profile.release.package.codex-core.codegen-units=16' -p codex-cli -p codex-app-server -p codex-code-mode-host --bin codex --bin codex-app-server --bin codex-code-mode-host
```

These commands describe compilation; they do not claim bit-for-bit reproducibility or successful execution on every machine. Verify all three `--version` outputs, the full source freeze and actual executable hashes, then follow [README.md](README.md) to create a new complete package using official `0.167.0` helpers. Packaging does not install or switch a running server. Preserve existing user data and keep a previous complete package for recovery; operational replacement requires separate authorization.

## 한국어

패키지의 `source/arcvera-core-source.zip`에는 Cargo 명세·잠금파일, 프로젝트 벤더 코드, 네이티브 소스 참조, 빌드 스크립트, 패키징 도구와 라이선스 고지를 포함한 정확한 커밋의 프로젝트 트리가 들어 있습니다. `SOURCE.md`는 실행 파일의 소스 커밋을 식별합니다. 실제 컴파일러·의존 입력·프로필·실행 파일 해시·미검증 사항은 최종 릴리스 검증 부록을 따릅니다. 배포 전 의존성·네이티브 구성요소의 대응소스 완결성과 권리를 별도로 검증해야 하며, 프로젝트 소스 압축파일만으로 그 검증을 대신하지 않습니다.

격리된 Windows x86-64 빌드 폴더에서 MSVC x64 C/C++ 빌드 도구·Windows SDK, Rust `1.99.1-x86_64-pc-windows-msvc`, Python 3.11 이상, Git, CMake와 잠금 의존성이 요구하는 네이티브 도구를 준비합니다. 공식 기반 `rust-toolchain.toml`은 과거 입력으로 보존하고 위 명령에서 릴리스 컴파일러를 명시적으로 선택합니다. `Cargo.lock`을 유지하며 인증·사용자 프로필·운영 로그를 복사하지 않습니다. 별도 네이티브 도우미는 `third_party/voice/sources.json`과 고지를 확인합니다.

압축을 푼 `codex-rs` 폴더에서 x64 MSVC 환경이 설정된 Developer PowerShell로 위 명령을 실행합니다. 최종 부록에 후속 검증된 override가 있으면 그 설정을 따릅니다. 자원 가드와 캐시는 해당 빌드에만 적용하고 시스템 전역 메모리 설정을 바꾸지 않습니다. 위 명령은 컴파일 절차이며 모든 환경의 성공이나 바이트 단위 재현을 보장하지 않습니다.

실제 실행 파일 세 개의 `--version`·전체 소스 동결·해시를 확인한 뒤 [README.md](README.md)에 따라 공식 `0.167.0` 도우미와 새 패키지를 만듭니다. 패키징은 설치나 실행 중 서버 교체를 수행하지 않습니다. 원 사용자 데이터와 이전 전체 패키지를 보존하며 운영 교체는 별도 승인을 따릅니다.
