# Licensing / 라이선스

## English

Arcvera-Core 1.0.0 and all subsequent project versions are licensed under the GNU Affero General Public License, version 3 or (at your option) any later version (`AGPL-3.0-or-later`). See [LICENSE](LICENSE) for the complete terms. This continuing release policy covers maintenance and major versions.

This work incorporates Apache-2.0 software. The original repository license is preserved byte for byte in [LICENSE-APACHE-2.0](LICENSE-APACHE-2.0). The original [NOTICE](NOTICE), [upstream license](upstream/LICENSE), [upstream notices](upstream/NOTICE), file copyright notices and [modification notices](MODIFICATIONS.md) remain applicable. Original Apache notices do not license the complete Arcvera-Core derivative under Apache-2.0. Independently licensed dependencies, vendored code, examples and resources retain their own licenses. This policy does not relicense third-party material or proprietary OpenAI Desktop software.

The Windows release package includes the exact committed project source under `source/`, together with a `SOURCE.md` identifying the full commit, archive and build instructions. The public repository is <https://github.com/jaeseongs95/Arcvera-Core>. Release source must match the executable build commit. Keep the source archive, source offer, licenses and notices when redistributing a package. Supply the complete corresponding source required by the license, including changes and scripts needed to control compilation and installation; a link to a different revision is insufficient. See [source build](docs/source-import.md) and [Windows packaging](tools/windows-cli/README.md).

When running a modified version for users interacting remotely through a computer network, prominently offer those users access to its corresponding source at no charge through a standard or customary means. Operators of a client or service exposing Core must make that offer visible in the interface those users actually access and identify their deployed revision and changes. A local file invisible to network users is insufficient. This obligation also applies to later modified versions; preserve the source offer when integrating Core into another application. See section 13 of LICENSE. Packaging and repository publication alone do not verify a deployed service's offer.

## 한국어

Arcvera-Core 1.0.0과 이후 모든 프로젝트 버전에는 GNU Affero General Public License 제3판 또는 이용자가 선택하는 그 이후 판(`AGPL-3.0-or-later`)을 적용합니다. 전문은 [LICENSE](LICENSE)에 있습니다. 유지보수 버전과 주 버전 변경에도 이 정책을 유지합니다.

이 프로젝트에는 Apache-2.0 소프트웨어가 포함됩니다. 기존 저장소 라이선스는 [LICENSE-APACHE-2.0](LICENSE-APACHE-2.0)에 원래 바이트 그대로 보존하며, 기존 [NOTICE](NOTICE), [공식 기반 라이선스](upstream/LICENSE)·[고지](upstream/NOTICE), 파일별 저작권과 [수정 고지](MODIFICATIONS.md)를 유지합니다. 원 Apache 고지가 Arcvera-Core 파생물 전체에 Apache-2.0을 적용한다는 뜻은 아닙니다. 별도 라이선스의 의존성·벤더 코드·예제·리소스는 각 라이선스를 유지하며, 이 정책으로 제삼자 자료나 독점 OpenAI Desktop 소프트웨어의 라이선스를 변경하지 않습니다.

Windows 배포 패키지는 `source/`에 정확한 커밋의 프로젝트 소스를 포함하고, `SOURCE.md`에 전체 커밋·소스 압축파일·빌드 안내를 기록합니다. 공개 저장소는 <https://github.com/jaeseongs95/Arcvera-Core>입니다. 배포 소스는 실행 파일의 빌드 커밋과 일치해야 합니다. 재배포할 때 소스 압축파일·소스 제공 안내·라이선스·고지를 유지하고, 수정 내용과 컴파일·설치를 제어하는 스크립트를 포함해 라이선스가 요구하는 완전한 대응소스를 제공해야 합니다. 다른 리비전의 링크만 제공하는 것으로 대신할 수 없습니다. [소스 빌드](docs/source-import.md)와 [Windows 패키징](tools/windows-cli/README.md)을 따릅니다.

수정 버전을 컴퓨터 네트워크로 원격 이용하는 사용자가 있다면, 표준적이거나 통상적인 방법으로 대응소스에 무료 접근할 수 있다는 안내를 그 사용자에게 잘 보이게 제공해야 합니다. Core를 노출하는 클라이언트·서비스 운영자는 사용자가 실제로 접근하는 화면에 이 안내를 표시하고, 운영 중인 리비전과 수정 내용을 식별해야 합니다. 원격 사용자가 볼 수 없는 로컬 파일만으로 대신할 수 없습니다. 후속 수정 버전에도 적용되며, 다른 애플리케이션에 통합할 때 소스 제공 안내를 유지해야 합니다. LICENSE 제13조를 따릅니다. 패키지와 저장소 공개만으로 운영 서비스의 안내를 검증한 것은 아닙니다.
# External dependencies / 외부 의존성 정책 (2026-11-13)

Microsoft runtime DLLs and installers are excluded from Arcvera distributions.
Users obtain the required runtime directly from Microsoft; see
the package's `RUNTIME.md` or the [official download guide](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). A dependency may be distributed only
when its redistribution conditions can be met alongside this package's terms.
Otherwise omit it and document installation from the official provider.
This policy does not remove the source, notices or other duties for bundled OSS.

Microsoft 런타임 DLL과 설치기는 Arcvera 배포물에서 제외합니다. 필요한
런타임은 사용자가 Microsoft에서 직접 취득하며
패키지의 `RUNTIME.md` 또는 [공식 다운로드 안내](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)를 따릅니다. 패키지의
조건과 함께 재배포 조건을 충족할 수 있는 의존성만 배포합니다. 충족할 수
없으면 해당 의존성을 제외하고 공식 제공자의 직접 설치 방법을 안내합니다.
동봉하는 오픈소스의 소스 제공·고지 등 의무는 계속 유지합니다.
