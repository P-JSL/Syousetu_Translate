# Syousetu Translate

**일본어 웹소설을 한국어로 번역하는 Rust 기반 데스크톱 프로그램입니다.**

웹소설 URL을 입력해 목차를 확인하고, 원하는 회차를 번역하여 TXT 파일로 저장할 수 있습니다. 기본 번역 엔진은 CPU에서 동작하는 **SMaLL-100 INT8 오프라인 모델**입니다. 번역에 Java, Python, GPU, ezTrans XP 또는 외부 번역 API가 필요하지 않습니다.

## 다운로드 및 실행

- **[최신 Windows x64 릴리스 다운로드](https://github.com/P-JSL/Syousetu_Translate/releases/latest)**
- 지원 환경: **Windows 10/11, 64비트**
- Windows 배포 ZIP에는 번역 모델과 실행 파일이 포함되어 있습니다.

1. 최신 릴리스에서 `SyousetuTranslate-Web-*-Windows-x64.zip` 파일을 다운로드합니다.
2. ZIP **전체를 같은 폴더에 압축 해제**합니다. `models` 폴더를 따로 이동하거나 삭제하지 마세요.
3. **`웹소설 번역기.bat`**을 실행합니다.
4. **웹소설** 탭에서 작품 URL을 입력하고 **목차 확인**을 선택합니다.
5. 번역할 시작·마지막 회차와 출력 파일을 지정한 다음 **번역 실행**을 누릅니다.
6. 작업 큐에서 진행률을 확인하고, 완료된 TXT 파일을 열어봅니다.

배포 ZIP에는 모델이 들어 있으므로 정상적인 압축 해제 후에는 별도 모델 다운로드가 필요하지 않습니다. **모델 준비 / 복구**는 모델이 누락되거나 손상된 경우에 사용합니다.

> 번역 연산 자체는 오프라인으로 실행됩니다. 웹소설의 목차·본문을 사이트에서 가져오려면 인터넷 연결이 필요합니다. 모델은 CPU와 메모리를 사용하므로 여유 RAM **2GB 이상**을 권장합니다.

## 주요 기능

- **지원 사이트:** Kakuyomu, Alphapolis, 소설가가 되자(Syosetu), Syosetu18, Hameln, Novelup
- **회차 선택:** 목차 조회, 시작·마지막 화 지정, 다중 작업 큐와 작업 순서 변경
- **TXT 저장:** 회차 순서를 유지하며 번역 결과를 파일에 순차 기록, 중단 시 완료된 내용 보존
- **교정 기억:** 직접 수정한 문장·용어를 로컬에 저장하고 이후 번역에 반영
- **번역 설정:** 원문 병기, 후리가나·댓글 포함, 출력 위치 및 파일명 설정
- **CPU 병렬 처리:** 기본 논리 작업자 32개, 작업 환경에 맞춰 조정 가능
- **로컬 모델:** 일본어에서 한국어로 직접 번역하며, 번역 문장을 외부 번역 API로 전송하지 않음

기존 출력 파일을 덮어쓰는 작업에서는 오류·중단에 대비해 `.backup` 파일을 사용합니다. 작업 실패 후 남은 백업은 확인한 다음 동일한 출력 경로를 다시 사용하세요.

### 교정 기억

GUI의 **교정 기억**에서 일본어 원문과 수정한 한국어 문장 또는 용어를 등록·수정·삭제할 수 있습니다. 교정 데이터는 로컬에 보관되며 **모델 가중치의 재학습을 의미하지 않습니다.** 자동 생성된 번역 결과를 승인 없이 학습 데이터로 저장하지 않습니다.

## 소스 코드 빌드

개발 환경에는 **Rust 및 Cargo**가 필요합니다.

```sh
# CLI 빌드
cargo build --locked --release

# GUI + CLI 빌드
cargo build --locked --release --features gui --bins

# 테스트 및 정적 검사
cargo test --locked --all-targets
cargo clippy --locked --all-targets
```

GUI는 `target/release/syousetu-gui`에서 실행합니다(Windows는 `.exe`). 두 실행 파일은 같은 폴더에 있어야 합니다. Windows에서는 저장소의 `웹소설 번역기.bat`도 사용할 수 있습니다. Linux GUI는 X11/OpenGL 실행 환경이 필요합니다.

소스 빌드에는 배포 ZIP과 달리 모델이 포함되지 않을 수 있습니다. 이 경우 다음 명령으로 준비합니다.

```sh
cargo run --locked --release -- models --prepare
cargo run --locked --release -- models --verify
```

## CLI 사용 예시

```sh
# 목차 확인
cargo run --locked --release -- inspect --url https://kakuyomu.jp/works/2912051603596422650

# 1~2화 번역 후 TXT 저장
cargo run --locked --release -- download --url https://kakuyomu.jp/works/2912051603596422650 --start 1 --end 2 --out novel.txt

# 로컬 TXT 번역
cargo run --locked --release -- file --input original.txt --out translated.txt

# 짧은 문장 번역
cargo run --locked --release -- translate --text "私は学生です。"

# 직접 수정한 번역 저장
cargo run --locked --release -- memory --learn --source "私は学生です。" --corrected "저는 학생입니다."
```

자주 사용하는 옵션은 `--jobs 1..256`(기본 32), `--parallel-mode auto|serial|parallel`, `--japanese`(원문 병기), `--model-dir PATH`, `--memory-path PATH`입니다. 작업자를 늘려도 항상 더 빠르지는 않으며 메모리 사용량이 증가할 수 있습니다. 전체 옵션은 `cargo run -- --help`에서 확인할 수 있습니다.

## 제한 사항

- 자동 사이트 접근이 제한되거나 페이지 구조가 변경되면 목차·본문 다운로드에 실패할 수 있습니다. 로그인, CAPTCHA, 유료 콘텐츠 또는 연령 확인을 우회하지 않습니다.
- 경량 번역 모델은 고유명사, 구어체, 문맥이 복잡한 문장에서 오역할 수 있습니다. 번역 결과는 검토가 필요합니다.
- 기본 `internal` 엔진은 ezTrans XP와 동일한 번역 결과를 보장하지 않습니다. `passthrough`는 원문 보존을 위한 테스트 옵션이며 번역 기능이 아닙니다.
- 기존 ezTrans DLL 및 실험적 호환 기능은 별도의 `legacy` 빌드 범위입니다. **상용 엔진 자체를 Rust로 완전히 재구현한 것은 아닙니다.**
- 사이트별 전체 작품 다운로드와 모든 Windows 환경에서의 호환성이 완전히 검증된 것은 아닙니다.

## 자세한 문서

- [Windows GUI 사용 안내](docs/windows-web-client.md)
- [오프라인 모델 및 교정 기억](docs/offline-engine.md)
- [프로젝트 구조](docs/architecture.md)
- [CPU 병렬 처리 및 성능 검증](docs/cpu-batching.md)
- [기존 클라이언트 이식 범위와 한계](docs/porting-status.md)
- [서드파티 라이선스·고지](THIRD_PARTY_NOTICES.md)

프로젝트 자체의 라이선스는 `Cargo.toml`에 명시된 **MIT**입니다. 모델과 외부 구성요소에는 각각의 라이선스가 적용되며, 상용 DLL·사전 등 권한이 없는 파일은 배포에 포함하지 않습니다.
