# Syousetu Translate

**일본어 웹소설을 한국어로 번역하고 TXT 파일로 저장하는 프로그램입니다.**

Rust로 개발했으며, 기본 번역 엔진은 **SMaLL-100 INT8**입니다. 번역은 PC에서 오프라인으로 처리되며 Java, Python, GPU, ezTrans XP 또는 외부 번역 API가 필요하지 않습니다.

## 다운로드

**[Windows 64비트 최신 버전 다운로드](https://github.com/P-JSL/Syousetu_Translate/releases/latest)**

- **지원 OS:** Windows 10 / 11 (64비트)
- **배포 형식:** 실행 파일과 번역 모델이 포함된 ZIP
- **권장 환경:** 번역 작업을 위한 여유 메모리 2GB 이상

## 사용 방법

1. 최신 릴리스에서 `SyousetuTranslate-Web-*-Windows-x64.zip`을 다운로드합니다.
2. ZIP을 **전체 압축 해제**합니다.
3. 압축 해제한 폴더의 **`웹소설 번역기.bat`**을 실행합니다.
4. **웹소설** 탭에 작품 URL을 입력한 뒤 **목차 확인**을 누릅니다.
5. 번역할 회차와 TXT 저장 위치를 지정하고 **번역 실행**을 누릅니다.

배포 ZIP에는 모델이 포함되어 있으므로 처음 실행할 때 별도 다운로드가 필요하지 않습니다. `models` 폴더는 실행 파일과 함께 유지해 주세요.

## 주요 기능

| 기능 | 설명 |
| --- | --- |
| 웹소설 번역 | URL로 목차를 불러오고 원하는 회차만 선택하여 번역 |
| 지원 사이트 | Kakuyomu, Alphapolis, Syosetu, Syosetu18, Hameln, Novelup |
| 오프라인 번역 | 일본어를 한국어로 직접 번역하는 로컬 INT8 모델 |
| 교정 기억 | 사용자가 수정한 문장·용어를 저장하여 이후 작업에 반영 |
| 작업 관리 | 진행률 확인, 대기 작업 순서 변경 및 삭제 |
| TXT 출력 | 번역 결과를 회차 순서대로 저장하고 작업 중에도 파일에 기록 |
| 출력 설정 | 원문 병기, 후리가나·댓글 포함 여부, 저장 위치·파일명 설정 |

**교정 기억**은 로컬 번역 메모리이며 모델을 재학습하는 기능은 아닙니다. 자동 생성된 번역을 임의로 학습 데이터에 추가하지 않습니다.

## 알아두세요

- **인터넷 연결:** 번역 연산은 오프라인이지만 웹소설 목차와 본문을 다운로드할 때는 사이트 접속이 필요합니다.
- **중단·실패:** 이미 기록된 TXT 내용은 유지됩니다. 기존 파일을 덮어쓰던 작업은 원본 보호를 위해 `.backup` 파일을 사용할 수 있습니다.
- **모델 오류:** 모델 파일이 누락되거나 손상되었다면 GUI의 **모델 준비 / 복구**를 이용하세요.
- **번역 품질:** 고유명사, 속어, 긴 문장 등에서 오역이 발생할 수 있으며 ezTrans XP와 동일한 결과를 보장하지 않습니다.
- **사이트 제한:** 로그인, 유료 콘텐츠, CAPTCHA 또는 연령 확인을 우회하지 않습니다. 사이트 구조 변경에 따라 일부 작품을 가져오지 못할 수 있습니다.

## 개발자용 빌드 및 CLI

소스에서 빌드하려면 Rust와 Cargo가 필요합니다. Windows 배포 ZIP과 달리 소스 빌드에는 모델이 자동 포함되지 않습니다.

```bash
# GUI 및 CLI 빌드
cargo build --locked --release --features gui --bins

# 소스 빌드 시 모델 준비 및 검증
cargo run --locked --release -- models --prepare
cargo run --locked --release -- models --verify

# 목차 조회
cargo run --locked --release -- inspect --url https://kakuyomu.jp/works/2912051603596422650

# 지정 회차를 번역하여 TXT로 저장
cargo run --locked --release -- download --url https://kakuyomu.jp/works/2912051603596422650 --start 1 --end 2 --out novel.txt

# 로컬 파일 번역
cargo run --locked --release -- file --input original.txt --out translated.txt

# 테스트 및 정적 분석
cargo test --locked --all-targets
cargo clippy --locked --all-targets
```

GUI 실행 파일은 `target/release/syousetu-gui`(Windows: `.exe`)입니다. CLI 전체 옵션은 `cargo run -- --help`로 확인할 수 있습니다. 기본 병렬 작업 용량은 32이며, `--jobs`로 조정할 수 있습니다. 작업자 수가 많을수록 반드시 빨라지는 것은 아닙니다.

## 상세 문서

- [Windows 사용 안내](docs/windows-web-client.md)
- [번역 모델 및 교정 기억](docs/offline-engine.md)
- [프로젝트 구조](docs/architecture.md)
- [성능 및 병렬 처리](docs/cpu-batching.md)
- [기존 프로그램 이식 범위와 제한 사항](docs/porting-status.md)
- [서드파티 라이선스](THIRD_PARTY_NOTICES.md)

## 라이선스

프로젝트 코드는 **MIT 라이선스**를 따릅니다. 번역 모델과 외부 구성요소에는 별도의 라이선스가 적용될 수 있습니다. 상용 DLL과 사전은 배포에 포함하지 않습니다.
