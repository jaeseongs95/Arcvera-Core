# Windows runtime prerequisite

Arcvera-Core does not distribute Microsoft runtime DLLs or runtime installers.
Before using native voice on Windows, obtain the latest supported Microsoft
Visual C++ v14 Redistributable from the
[official Microsoft download page](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
For the `windows-x86_64` package, select **X64**. This package's voice libraries
were prepared with MSVC 14.50; use v14.50 or later, subject to Microsoft's supported
Windows versions and license terms. Microsoft manages the runtime's installation
and updates separately. Arcvera does not download or install it automatically.

If Windows reports a missing `VCRUNTIME140.dll` or another C/C++ runtime DLL,
install or repair the official redistributable and restart Arcvera. Do not copy
individual DLLs from third-party download sites into the package directory.
The installer must be obtained directly from Microsoft by the user; it is not
included in this archive. Microsoft runtime installation is separate from the
Arcvera software license. See `SOURCE.md` and the included third-party notices.

Distributors must not add dependencies without a verified redistribution basis
compatible with this package. If redistribution cannot meet the applicable
terms, omit that dependency and document direct installation from its official
provider. Preserve source, attribution and other duties for distributed OSS.

# Windows 런타임 사전 설치

Arcvera-Core는 Microsoft 런타임 DLL과 설치기를 배포하지 않습니다.
Windows 네이티브 음성을 사용하기 전에
[Microsoft 공식 다운로드 안내](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)에서
최신 지원 Microsoft Visual C++ v14 Redistributable을 직접 받으십시오.
`windows-x86_64` 패키지는 **X64**를 선택합니다. 음성 라이브러리는
MSVC 14.50으로 준비했으므로 Microsoft가 지원하는 Windows 버전과
사용 조건을 확인하고 v14.50 이상을 설치하십시오. 런타임 설치와 업데이트는
Microsoft 경로로 별도 관리하며 Arcvera가 자동으로 다운로드하거나 설치하지 않습니다.

`VCRUNTIME140.dll` 등 C/C++ 런타임 DLL 누락 오류가 표시되면 공식
재배포 패키지를 설치 또는 복구하고 Arcvera를 다시 실행하십시오. 개별 DLL을
제삼자 다운로드 사이트에서 받아 패키지 폴더에 복사하지 마십시오.
설치기는 사용자가 Microsoft에서 직접 취득하며 이 압축파일에는 포함되지 않습니다.
Microsoft 런타임 설치에는 Arcvera 라이선스와 별도의 사용 조건이 적용됩니다.
`SOURCE.md`와 함께 제공한 제삼자 고지를 확인하십시오.

배포자는 패키지에 적용되는 조건과 양립하는 재배포 근거를 확인한 의존성만
포함해야 합니다. 재배포 조건을 충족할 수 없다면 해당 의존성을 제외하고
공식 제공자로부터 직접 설치하는 방법을 안내하십시오. 배포하는 오픈소스의
소스 제공·저작권 표시 등 의무는 계속 준수해야 합니다.
