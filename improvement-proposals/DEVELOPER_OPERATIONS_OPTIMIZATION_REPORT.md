# 5. Syousetu_Translate — 일본어 웹소설 번역기

**저장소:** [P-JSL/Syousetu_Translate](https://github.com/P-JSL/Syousetu_Translate)  
**소스 분석 기준:** `main` / [`612f743d9aeec166a039d865e332d58fb92b5d1b`](https://github.com/P-JSL/Syousetu_Translate/commit/612f743d9aeec166a039d865e332d58fb92b5d1b)  
**보고서 초판:** 2026-11-11 (KST) · **교차 검증 갱신:** 2026-11-12 (KST)  
**대상:** 개발자 · 운영자 · 릴리스 담당자  
**누적 기존 IMP 개선 과제:** 8개 (초판 5개 포함, 별도 비성능 Q 제어/검토 항목 10개)  
**상태:** 분석·구현 제안만 작성. 실제 소스 최적화·동적 성능 측정·프로그램 배포는 수행하지 않음.  
> 문서 내 장·절 번호는 원래 6개 저장소 통합 보고서의 번호를 보존한 것입니다. 코드 근거 링크는 작성 시점의 커밋으로 고정했습니다.

## 5.1 실행 경로와 책임 분리

```text
웹소설 번역기.bat → syousetu-gui.exe
  → bin/desktop/app.rs (UI 상태 / 탭)
  → bin/desktop/novel.rs (URL, 회차, 출력, 실행 요청)
  → jobs/mod.rs (대기열·설정 스냅샷)
  → jobs/process.rs (형제 syousetu-translate CLI 프로세스 기동)
     → cli/mod.rs / commands/novel.rs
        → novel/site/* (URL 식별, 목차·본문 파싱)
        → novel/network.rs (허용된 HTTP 접근·지연·재시도)
        → commands/novel/source.rs (회차 선행 다운로드·순서 보존)
        → offline/* (SMaLL-100 INT8 번역)
        → learning/mod.rs (문장·용어 교정 기억)
        → 출력 TXT와 백업 (storage / output)
```

GUI는 자체적으로 모든 번역을 수행하지 않습니다. [src/jobs/process.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/jobs/process.rs#L93)에서 각 작업 설정을 **임시 `config.properties`**로 저장하고, 형제 CLI 프로세스를 실행해 stdout/stderr로 진행·오류를 수집합니다. GUI가 열려 있어도 CLI 실행 파일이 없으면 작업을 수행할 수 없습니다. [src/jobs/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/jobs/mod.rs#L62)는 큐 등록 시 설정을 복사하므로 **등록 후 GUI 설정 변경이 기존 작업을 바꾸지 않습니다.**

[src/bin/desktop/novel.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/bin/desktop/novel.rs#L11)은 `목차 확인`과 `번역 실행`을 명확하게 구분합니다. 마지막 화 `0`은 끝까지라는 의미이며, 출력 파일을 비운 상태에서 자동 파일명 옵션을 사용하거나 저장 대화상자를 띄우는 분기가 있습니다.

## 5.2 핵심 모듈 해설

| 모듈 | 책임 | 예외/데이터 정합성 포인트 |
|---|---|---|
| [src/cli/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/cli/mod.rs#L15) | 명령 라우팅: `inspect`, `download`, `file`, `models`, `memory`, `translate` 등 | `legacy` 기능은 선택 빌드 구분 |
| [src/cli/commands/novel.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/cli/commands/novel.rs#L95) | 작품 로딩·회차 범위 계산·번역 세션·출력 | 부분 성공분 기록·실패 시 출력 보존 |
| [src/cli/commands/novel/source.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/cli/commands/novel/source.rs#L60) | 소스 HTML 선행 다운로드, 순서 있는 bounded 채널 | 뒤 회차가 먼저 끝나도 순서가 뒤집히지 않도록 제한 |
| [src/novel/network.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/novel/network.rs#L38) | 요청 간 대기·재시도·응답 크기·HTTP 상태·리다이렉트 검증 | 인증/도전 페이지를 우회하지 않고 명시적 실패 처리 |
| [src/novel/site/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/novel/site/mod.rs#L1) | 사이트별 어댑터와 DOM 파서 | HTML 구조 변경·페이징 누락·목차 완전성 |
| [src/offline/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/mod.rs#L18) | 오프라인 번역 모델·스케줄러·토크나이저 | CPU·메모리 사용량, 문장 순서, 모델 버전 |
| [src/learning/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/learning/mod.rs#L74) | 사용자 승인 교정 및 용어 치환 | 파일 잠금·재읽기·원자적 저장, 손상 파일 보호 |
| [src/models/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/models/mod.rs#L25) | 모델 상태·준비·검증 | '상태 표시'는 빠른 크기 검사, 준비/로딩은 전체 해시 검증 |

**번역 처리:** 사이트 접속은 네트워크가 필요하지만 로컬 TXT 변환과 이미 준비된 INT8 추론은 오프라인 가능합니다. [src/novel/network.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/novel/network.rs#L55)는 HTTP 401/403/404/429를 일반 성공으로 처리하지 않고 사이트 접근·제한 메시지로 구분합니다. 응답 크기·타임아웃·교차 출처 리다이렉트도 제어합니다.

**교정 기억:** [src/learning/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/learning/mod.rs#L110)은 원문 일치 문장 교정을 먼저 적용하고, [src/learning/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/learning/mod.rs#L143)은 일본어 원문에 해당 용어가 있을 경우에만 한국어 표현을 교체합니다. [src/learning/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/learning/mod.rs#L183)은 잠금 → 최신 데이터 다시 읽기 → 편집 → 원자적 저장으로 다중 GUI 인스턴스 충돌을 줄입니다. 손상된 교정 파일은 자동 초기화하지 않는 경로가 있으며, **교정 기억은 모델 가중치를 다시 학습하는 기능이 아닙니다.**

## 5.3 확인된 이식성 문제와 진단

[src/config/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/config/mod.rs#L63)의 `download_directory()`에서 `download_dir`가 비어 있고 Windows라면 특정 사용자 홈의 `C:\Users\rabbit\OneDrive\문서`를 기본 경로로 반환합니다. **다른 사용자 PC에서 존재하지 않을 수 있는 경로**라는 점은 코드로 확인되었습니다. 실제 쓰기 실패는 해당 PC 환경에 따라 달라집니다. 운영 표준은 **최초 실행 시 설정 → 다운로드 기본 폴더를 실제 사용자 문서 폴더로 지정**하는 것입니다. 개발 개선안은 실행 사용자 환경의 표준 폴더 또는 사용자가 지정한 폴더를 이용하도록 기본 경로 계산을 변경하고, 기존 설정이 있으면 우선 유지하는 방식입니다.

[src/models/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/models/mod.rs#L25)의 GUI용 모델 상태는 **파일 크기 중심 빠른 확인**이므로, 동일 크기의 손상 파일까지 '준비됨'으로 보일 가능성이 있습니다. 대신 실제 모델 준비·로드 경로에는 별도 SHA 검증이 있습니다. 운영자는 상태 문구만 근거로 정상 추론을 확정하면 안 됩니다.

## 5.4 운영 절차·장애 대응

1. **배포:** v0.3.5 Windows x64 릴리스 ZIP을 **전체 압축 해제**하고 `웹소설 번역기.bat`, `syousetu-gui.exe`, `syousetu-translate.exe`, `models/ja-ko/`를 같은 패키지 구조에 유지합니다.
2. **검증:** GUI에 모델 상태가 표시되는지, 목차 조회가 정상인지, 1~2화 수준의 **허가된 테스트 작품**을 로컬 TXT로 저장할 수 있는지 확인합니다.
3. **설정:** 다운로드 기본 폴더, 모델 폴더, 작업자 수(기본 논리 컨텍스트 32개), 자동/단일/병렬 처리, 원문 병기·후리가나·작가 코멘트 등을 환경에 맞게 저장합니다.
4. **실행:** URL → 목차 확인 → 시작/마지막 화 → 출력 경로 → 번역 실행 → 작업 큐 로그 → TXT의 회차 순서 확인.
5. **중단:** 저장된 TXT의 완료분과 덮어쓰기 이전 `.backup`을 먼저 보존합니다. 같은 경로에 재실행하기 전에 복구 여부를 확인합니다.
6. **모델 장애:** `models --verify` 실패면 권한·해시·남은 저장 공간을 점검하고 `models --prepare`로 복구합니다. 사내 프록시 인증서 문제는 Windows 신뢰 저장소 및 추가 PEM CA를 확인하되 검증을 해제하지 않습니다.

| 증상 | 소스 조사 출발점 | 첫 대응 |
|---|---|---|
| `GUI not found` 또는 BAT 미실행 | [웹소설 번역기.bat](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/%EC%9B%B9%EC%86%8C%EC%84%A4%20%EB%B2%88%EC%97%AD%EA%B8%B0.bat#L1) | BAT와 실행 파일 위치 확인; ZIP 전체 추출 |
| 작업 실패, CLI 출력 없음 | [src/jobs/process.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/jobs/process.rs#L93) | 형제 CLI 및 임시 설정 파일 생성/권한 확인 |
| 목차·회차 누락 | [src/novel/site/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/novel/site/mod.rs#L1) | 사이트 어댑터와 원본 HTML 구조·페이지 범위 비교 |
| 403·429 또는 접근 도전 | [src/novel/network.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/novel/network.rs#L121) | 사이트의 접근 조건 확인, 우회하지 않음 |
| 모델 누락·SHA 실패 | [src/models/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/models/mod.rs#L46) | `models --verify`, 모델 준비/복구 및 디스크 여유 공간 |
| 교정 내용이 사라짐/반영되지 않음 | [src/learning/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/learning/mod.rs#L74) | 교정 적용 ON, 파일 경로/손상·다른 작업의 설정 스냅샷 확인 |

### 개발자 검증 명령

```sh
cargo build --locked --release --features gui --bins
cargo test --locked --all-targets
cargo clippy --locked --all-targets
cargo run --locked --release -- models --verify
cargo run --locked --release -- inspect --url "<허가된 테스트 작품 URL>"
```

**실제 네트워크 테스트**는 사이트 약관과 접근 허가를 확인한 작품에서만 수행합니다. 새 사이트 어댑터를 추가할 때는 목차 페이지 일부 누락, 서브페이지 중복, 화 순서, 대량 회차 중도 실패, 파일 출력 경로를 함께 시험해야 합니다. 위 명령은 실행하지 않았습니다.

---

## 5.5 번역 모델·CPU 실행 속도 최적화 심층 검토 (추가)

**모델 분류:** `SMaLL-100`은 Encoder–Decoder 기반 신경망 번역 모델입니다. 일반적인 대화형 LLM API나 외부 GPU 서비스가 아니라 **로컬 CPU·Candle·양자화 가중치** 경로를 사용합니다. 따라서 이 저장소에만 **토큰 생성·KV 캐시·배치 추론·양자화 품질** 관점의 모델 속도 최적화가 직접 적용됩니다. LLM 일반론(예: KV 캐시를 단순히 켜기, 무조건 GPU로 변경)은 현재 코드와 맞지 않습니다.

### 5.5.1 기존에 구현된 가속·품질 보호

| 기존 구현 | 확인된 코드 | 분석 |
|---|---|---|
| `Q8_0`/INT8 공유 가중치 | [model/mod.rs L42-L53](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/mod.rs#L42) | 이미 양자화를 사용합니다. FP32→INT8 변경을 신규 대책으로 제안하는 것은 부정확합니다. 추가 4bit/혼합 정밀도는 품질·포맷·커널 재설계가 필요한 **장기 연구**입니다. |
| 선택적 공동 선형 연산, 독립 문장 Attention·KV | [layers/batch.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/layers/batch.rs#L1) | 문장 간 Attention 혼합·가짜 Padding 없이 공유 QKV/FFN 일부를 배치 처리합니다. 이미 batching을 사용하고 있습니다. |
| Iteration-level 동적 배치 | [runner/batched/mod.rs L91-L117](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/batched/mod.rs#L91) | 준비된 문장을 채우면서 종료된 문장 슬롯을 재사용합니다. 문장 길이의 편차를 처리할 설계가 있습니다. |
| CPU 제한 Rayon 풀·가중치/토크나이저 공유 | [runtime.rs L42-L80](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runtime.rs#L42) | 작업자 수와 실제 CPU 스레드 수를 분리합니다. 맹목적인 스레드 증가가 개선을 보장하지 않습니다. |
| 단일 vs 배치 자동 선택 | [scheduler.rs L5-L60](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/scheduler.rs#L5) | 첫 후보 문장으로 **결과 문자열 동일성**을 확인하고 배치가 단일 대비 10% 이상 빠른 경우에만 선택합니다. |
| 성능별 Dense/Fused 출력 프로젝션 | [model/projection.rs L70-L125](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/projection.rs#L70) | 행 수별 실제 측정·정확 출력 비교로 경로를 정합니다. 이미 'top-1만 계산' 후보가 구현돼 있습니다. |
| CPU F32 Attention fused kernel 및 스레드 로컬 scratch | [model/ops/attention.rs L25-L60](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/ops/attention.rs#L25) | 텐서 중간 객체를 줄이고 일부 큰 작업에서만 병렬화합니다. 커널을 바꿀 경우 수치·안전성 회귀 테스트가 필요합니다. |
| 중복 문장 캐시·고유 문장 배치 | [cache.rs L9-L46](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/cache.rs#L9), [scheduler.rs L92-L115](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/scheduler.rs#L92) | 1,024개 결과 캐시와 동일 입력의 배치 중복 제거가 있습니다. |

### 5.5.2 추가 후보 (가장 먼저 프로파일링)

| ID/우선 | 실제 비용 발생 위치 | 대안 및 트레이드오프 |
|---|---|---|
| **S-P1** | [layers/batch.rs L63-L68](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/layers/batch.rs#L63)의 self-attention KV는 **토큰 생성마다 `Tensor::cat(old, new)`**로 길어집니다. 기존 KV를 매 단계 재복사할 수 있으므로 전체 생성 길이 `T`에 대해 캐시 누적 복사량이 **O(T²)**에 가까워질 수 있습니다. | 용량을 선할당한 KV 버퍼에 새 토큰을 삽입하고 길이 뷰만 갱신하는 설계 또는 검증된 segmented/paged KV. Candle 텐서의 소유권·stride·contiguous 보장과 `attention.context()` 입력을 함께 바꿔야 합니다. **가장 유망한 추론 구조 후보이나 난도/정확성 위험이 높음**. |
| **S-P2** | [generation.rs L57-L65](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/generation.rs#L57)와 [L93-L103](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/generation.rs#L93)의 반복 3-gram 차단은 출력 전체에 `windows(3)`를 반복 적용해 단계별 집합을 재생성합니다. 배치 경로도 [batched/mod.rs L146-L149](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/batched/mod.rs#L146)에서 동일합니다. | 접두 2-token → 이미 등장한 다음-token 집합의 **증분 인덱스**를 유지하면 반복 조회/할당을 줄일 수 있습니다. 현 토큰 상한(최대 512)에서는 총 추론 대비 효과가 작을 수 있으므로 먼저 CPU profile로 확인해야 합니다. 차단 후보·동점 우선순위는 완전히 같아야 합니다. |
| **S-P1** | [scheduler.rs L26-L50](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/scheduler.rs#L26)의 자동 정책은 첫 샘플을 **단일 및 배치 양쪽으로 실제 추론**합니다. 선택적 초기 지연은 의도된 검증 비용입니다. | 모델·CPU·배치 크기별 성능 결정 캐시를 **검증된 버전 키**로만 저장하거나 더 작은 후보 묶음 비교. 반대로 첫 실행 속도보다 이후 수백 화 처리량이 중요할 수 있으므로 단기/장기 작업을 나눠 평가. 외부 프로파일 불일치 시 현재 자동 측정으로 복귀. |
| **S-P2** | [model/projection.rs L97-L125](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/projection.rs#L97)은 warmup에서 Dense와 Fused **둘 다 실행**해 결과 일치를 확인합니다. | 이미 정확성 보호가 있으므로 이를 삭제하기보다는 `inference-profile` 단계별 비용으로 warmup 비용과 steady-state 이익을 비교. 릴리스 성능만으로 무조건 Fused로 고정하면 안 됩니다. |
| **S-P2** | [cache.rs L41-L46](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/cache.rs#L41)의 `evict`는 캐시 순서 목록을 순회하며 `sources.contains(key)`를 사용합니다. 소스 문장 수 `S`, 캐시 항목 `C`에 대해 최악 **O(C×S)** 비교 비용입니다. | `sources`를 `HashSet<&str>`로 만들어 제거 검사 비용을 평균 상수로 전환. 제한 1,024이므로 미세 최적화이며 큰 모델 추론 비용을 줄이는 핵심 대책으로 과장하지 말 것. |
| **S-P2** | [layers/batch.rs L93-L116](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/layers/batch.rs#L93)은 새로운 Encoder 상태만 모아 Cross-Attention K/V를 계산해 캐시합니다. | 이미 캐시하므로 **매 토큰 Cross-KV 재계산**은 문제가 아닙니다. 다만 긴 문장·여러 활성 슬롯에 따른 캐시 메모리 점유와 각 레이어 메모리 상한은 계측할 필요가 있습니다. |
| **S-P3/연구** | 추가 저정밀 양자화/교체형 CPU 커널 | FP32 참조와 품질·출력 검증이 필요합니다. 4bit 재양자화, GPU 전환, speculative decoding 등은 **현재 구현 기반의 확정 개선안이 아닙니다**. 하드웨어·모델 가중치·라이선스·품질·배포 크기를 함께 검증하는 별도 실험으로 제한합니다. |

### 5.5.3 속도 실측 프레임워크 및 지표

이미 코드에 [추론 단계별 누적 계측](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/profiling.rs#L9)이 있습니다(`Encoder`, `Decoder`, `BatchLinear`, `BatchAttention`, `BatchFfn`, `Projection` 등). 또 [benchmark_offline.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/examples/benchmark_offline.rs#L10)는 **작업자 1/2/4**, 모델 로딩/추론 시간, 출력 SHA-256을 기록합니다. [benchmark_projection.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/examples/benchmark_projection.rs#L7)는 가중치 행렬의 Dense/Fused 비교를 지원합니다.

```sh
# 실제 모델 경로를 준비한 개발 PC에서 실행 (필요 시 환경/모델 옵션 확인)
cargo run --locked --release --features offline-model --example benchmark_offline
cargo run --locked --release --features offline-model --example benchmark_projection
cargo test --locked --release --features gui --lib --bins --tests
cargo clippy --locked --features gui --all-targets -- -D warnings
```

**동일 시험 입력:** (A) 1~2문장, (B) 평균 100~300자 문단, (C) 긴 문단과 대화체 혼합, (D) 50~100화 연속 처리. 재현용 원문/교정 기억 파일과 모델 SHA-256을 고정합니다. 각 경우 **cold/warm model load**, 첫 결과 지연(TTFT에 대응하는 첫 출력 시간), 출력 토큰/s, 문장/s, 회차/s, 각 단계별 추론 비중, CPU 이용률, Peak RSS, TXT 기록 지연을 구분 측정합니다. *원문 다운로드*는 별도 네트워크 타이머로 측정하여 추론 성능과 혼합하지 않습니다.

**합격:** 원본과 최종 출력이 byte-for-byte 같거나 명시적으로 승인된 허용오차/품질 차이를 만족, 문장 및 회차 순서 유지, 3-gram 반복 방지 정책 유지, 실패 시 부분 TXT/백업이 정상, 모델 SHA 검증 유지, 긴 입력에서도 메모리 상한과 취소 응답 유지. **중단/원복:** 빠르더라도 고유명사·조사·시제·인물 호칭이 달라지거나 원문 누락/회차 뒤섞임/모델 무결성 저하가 확인되면 채택하지 않습니다. **실측된 향상 배수는 아직 없습니다.**

## 5.6 번역기: LLM/SMaLL-100 추론 속도 개선 설계 (추가)

**모델 구분:** 내부 번역기는 자동회귀 신경망 **SMaLL-100** 기반이며 텍스트 생성형 LLM과 연산 특성이 유사한 디코더(인코더·디코더 구조) 추론 엔진을 갖습니다. 다른 5개 저장소에는 이와 같은 추론 경로가 없습니다. 기존 Q8_0 양자화·Rayon 작업자 풀·배치·Fused Attention·투사 자동 선택이 **이미 적용**되어 있습니다. 신규 이점은 프로파일로 증명해야 합니다.

### IMP-S01 · KV 캐시의 반복 `Tensor::cat` 제거 연구 (R / 최우선 계측)

- **위치:** `src/offline/model/layers/batch.rs`의 decoder self-attention 캐시 갱신. [근거 L44-L69](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/layers/batch.rs#L44). 현재 새 토큰마다 이전 `old_k/old_v`와 새 `k/v`를 `Tensor::cat()`으로 잇습니다.
- **원인:** 길이가 `t`인 K/V 캐시를 매 토큰 새 텐서로 복사한다면 누적 메모리 복사 총량은 `1+2+...+T = O(T²)`입니다. 실제 Candle 내부 구현·텐서 참조/메모리 사용을 측정하기 전, **복사량의 추정치와 실제 병목을 혼동하면 안 됩니다**.
- **구현 설계:** ① `inference-profile`에서 `decode_rows`, attention, KV `cat` 구간을 각각 측정하고 sequence 길이 32/64/128/256/512에 따른 `ms/token`, 메모리 및 캐시 복사 바이트를 기록합니다. ② 실험용 `DecoderKvCache`에 물리 capacity, 유효 길이 `len`, 한 토큰 쓰기(offset=`len`) 연산을 정의합니다. ③ 각 runner/각 layer마다 **독립 K/V 캐시**를 유지하고, 실제 attention에는 `0..len` 유효 구간만 노출합니다. ④ 초기 capacity는 현재 `limit` 범위를 상한으로 점검하고, 실패/재사용/문장 전환에서 `len=0` 및 캐시 초기화가 확실히 수행되게 합니다. ⑤ view, stride, 연산자 지원 및 쓰기 가능 여부가 Candle 모델과 맞지 않으면 **paged-block 저장 방식**을 별도 프로토타입으로 검증하고 성급히 대체하지 않습니다.
- **불변 계약:** `Q8_0` 모델 가중치·디코더 위치·forced Korean prefix·EOS·repeat block, 각 문장 KV 격리, serial/batch **토큰별 결과 일치**, 번역 메모리/품질이 동일해야 합니다. `[0, len)` 외 공간이 Attention에 노출되면 **수치가 달라지므로 즉시 반려**합니다.
- **롤백:** 실측 `ms/token`·peak RSS 이득이 없거나 Tensor 연산에서 메모리 오염·다른 문장의 KV 혼입이 발견되면 원본 `Tensor::cat` 경로를 유지합니다. 런타임 feature flag로 새 경로를 처음에는 실험 실행에서만 사용합니다.

### IMP-S02 · 3-gram 반복 탐색을 증분 자료구조로 바꾸기 (P1)

- **위치:** `src/offline/runner/generation.rs`의 `repeated_trigram_tokens()` 및 `src/offline/runner/batched/mod.rs`의 각 active state. [근거 L93-L103](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/generation.rs#L93), [배치 호출 L146-L149](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/batched/mod.rs#L146).
- **원인:** 매 디코딩 단계마다 출력 전체의 `windows(3)`을 재스캔해 마지막 두 토큰에 이어지면 안 되는 토큰을 찾습니다. 출력 길이 T 동안 기존 로직은 최악 `O(T²)` 경향이 있습니다.
- **구현 방법:** 문장별 `HashMap<(u32,u32), HashSet<u32>>`로 과거 3-gram을 보관합니다. 다음 토큰 선택 직전에는 `output.last_two()`를 키로 금지 집합을 조회하고, 토큰 `next`를 확정할 때 **이전 마지막 두 토큰 `(a,b)`에 대해 `map[(a,b)].insert(next)`**를 수행합니다. 출력이 2토큰 미만이면 빈 금지 집합을 사용합니다. serial과 batch 모두 **동일 상태 객체** 또는 동일 helper를 사용하되 각 문장의 데이터를 분리합니다.
- **테스트:** 시작 토큰 0/1/2개, 동일 토큰 반복, EOS 직전, 긴 토큰 배열, 여러 문장 interleave에서 기존 함수의 금지 집합과 **매 단계 완전 동일**한지 property test를 실행합니다. 이어서 실모델 출력의 각 토큰 ID·문장 SHA 비교. 문자열 출력뿐 아니라 **차단 집합 동일성**을 증명해야 합니다.
- **기대 효과 경계:** 차단 집합 계산 비용은 줄일 수 있으나 전체 시간은 모델 투사/GEMM이 지배할 수 있습니다. 이 경로의 시간 비중이 미미하면 추가 메모리 비용 때문에 적용하지 않습니다.

### IMP-S03 · 교정 캐시의 `evict` 비용 개선 (P2 / 낮은 위험)

- **위치:** `src/offline/cache.rs`의 `Cache::evict()` [근거 L41-L46](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/cache.rs#L41). 현재 `order.retain(|key| !sources.contains(key))`의 `sources.contains()`는 후보 크기에 비례하는 반복 탐색입니다.
- **구현:** 현재 `sources`에 중복이 있어도 같은 의미로 `HashSet<&str>`를 한 번 만들고, `values.remove()` 및 `order.retain()`에서 동일 집합을 사용합니다. 캐시 용량 `1024` 및 삽입/제거 순서는 그대로 유지합니다.
- **개념 예시(현재 프로젝트 직접 적용 코드 아님):**

```rust
let victims: std::collections::HashSet<&str> = sources.iter().map(String::as_str).collect();
for key in &victims { self.values.remove(*key); }
self.order.retain(|key| !victims.contains(key.as_str()));
```

- **검증:** `cache.rs` 단위 테스트에 중복 입력·없는 키·LRU 순서·상한·빈 리스트를 추가합니다. `evict`가 잦을 때만 개선 가능하며, 1~4개 삭제 상황에서 HashSet 생성 자체의 오버헤드와 비교합니다.

### IMP-S04 · 시작 지연: 자동 단일/배치 2회 시험 비용 제어 (P1)

- **위치:** `src/offline/scheduler.rs`의 `select_parallel()` [근거 L5-L60](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/scheduler.rs#L5). 현재 첫 본문에서 표본을 단일로 번역한 다음 **배치로 같은 표본을 재번역**해, 출력 일치 및 배치가 10% 이상 빠른지 확인하고 방식을 선택합니다.
- **방법:** ① `model_load_ms`, `serial_probe_ms`, `batch_probe_ms`, 첫 줄/첫 화 출력 완료 시각을 구분 계측합니다. ② 본문 길이·작업자 수별로 표본 4/8/16개 성능을 재현하고 샘플 수 감소가 올바른 방식을 선택하는지 점검합니다. ③ 동일 실행에서 결정한 정책은 현재처럼 재사용하고, **모델 교체·CPU 코어 수·병렬 정책 변경에는 결과를 재사용하지 않습니다**. ④ 첫 줄 표시가 중요하다면 시험을 첫 완료 뒤로 지연하는 방식도 고려할 수 있지만 이는 **출력/진행 이벤트의 시점 변경**이므로 별도 UX 승인 필요입니다.
- **정확성 가드:** `ParallelPolicy::Auto`의 현재 원칙인 **출력 완전 일치 + 배치 성능 10% 우위**를 임의로 완화하면 안 됩니다. 샘플이 짧아 CPU 추론의 전체 특성을 대표하지 못할 수 있으므로 중간 길이·긴 문장·혼합 일본어 본문에서 추가 비교.

### IMP-S05 · 배치 구성/메모리 할당과 모델 로드 개선 (P2)

- **위치:** [runner/batched/mod.rs L119-L169](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/batched/mod.rs#L119), [runtime.rs L41-L82](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runtime.rs#L41), [projection.rs L70-L126](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/projection.rs#L70).
- **방법:** 활성 slot 매 iteration의 `inputs`, `past`, `encoders`, `blocked`, `models` 소규모 벡터 재할당 횟수를 계측하고 스코프 생존이 허용되면 `Vec::clear()` + `reserve()`로 재사용합니다. `Arc`와 텐서 clone이 **얕은 핸들 복사인지 실제 데이터 복사인지** 계측하기 전 'clone 제거'를 단정적으로 제안하지 않습니다. 토크나이저/가중치는 이미 여러 runner 간 공유하므로 중복 인스턴스를 만들지 않습니다. Projection은 자체 dense/fused 프로빙이 있으므로 무조건 fused 고정 금지.
- **검증:** 동일 모델 SHA/같은 본문 5회 이상, CPU 1/2/4/8, batch 1/4/16/32를 분리하고 `ms/token`, `tokens/s`, 첫 출력, `malloc/realloc` 수, peak RSS를 측정합니다. [기존 벤치마크](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/examples/benchmark_offline.rs#L10)는 결과 SHA 일치를 확인합니다.

### 모델 성능 실험 예시·롤백

```sh
# 모델 파일이 준비된 PC에서, 실제 현재 Cargo feature 기준 실행
cargo test --locked --release --features gui --lib --bins --tests
cargo clippy --locked --features gui --all-targets -- -D warnings
# 환경변수 SYOUSETU_MODEL_DIR로 모델 위치를 지정해 예제 실행
# Linux/macOS: export SYOUSETU_MODEL_DIR=/path/to/model-root
# Windows PowerShell: $env:SYOUSETU_MODEL_DIR="C:\models\syousetu"
cargo run --locked --release --features offline-model --example benchmark_offline
cargo run --locked --release --features offline-model --example benchmark_projection
```

예제는 `-- <경로>` 인자를 받지 않고 `models::default_dir()`을 사용하며, `benchmark_offline.rs`는 `SYOUSETU_BENCH_INPUT` 환경변수로 고정 입력을 받을 수 있습니다. 모델 경로의 실제 환경 변수 연결은 `models/paths.rs` 구현과 빌드 구성에 맞춰 확인해야 합니다. 표준 성공 조건은 **토큰 ID·완성 한국어 문장·화 단위 TXT·교정 기억 적용 결과 SHA 전부 일치**, 모델 파손/중단/백업 시나리오 및 최대 RSS 상한 이탈 없음입니다. 모델 자체 교체·전문학습·GPU 도입은 성능 튜닝이 아니라 **추가 의존성·품질·배포 계약이 바뀌는 별도 프로젝트**입니다.


## 5.7 추가 보완 과제 — 실모델 품질 게이트·출력 I/O·배포 재현성 (2026-11-11)

IMP-S01~S05는 연산 병목 후보입니다. 추가로 **실제 INT8 모델이 설치된 환경에서 결과가 동일함을 증명하는 절차**, 파일 쓰기 빈도, 릴리스 자산 무결성을 별도로 관리해야 합니다.

### IMP-S06 · 일반 CI와 실제 모델 통합 검증을 명확히 분리 (P1)

- **확인 근거:** [tests/offline_model.rs L1-L10](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/tests/offline_model.rs#L1-L10)은 모델 다운로드 없이 실행하는 일반 CI와 달리 고정 모델 파일이 필요해 테스트를 ignored로 정의합니다. [tests/quantized_cpu.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/tests/quantized_cpu.rs)는 정량화 연산을 독립 검증합니다.
- **개선 절차:** ① 개발용 CI는 현재의 단위·정적 검사 유지. ② 변경된 커널·캐시·배치 관련 PR/릴리스 후보만 고정 모델 SHA가 있는 별도 실행 환경에서 ignored 통합 테스트 및 한국어 원문 골든 코퍼스를 수행. ③ 단일/병렬, 교정 메모리 사용, 화 순서, 긴 입력, 모델 없는 상태를 별도 결과로 기록. ④ 비밀 경로·개인 소설 본문은 결과에 기록하지 않습니다.
- **검증/원복:** 모델 준비 환경에서 cargo test --locked --release --features offline-model --test offline_model -- --ignored --nocapture 실행을 우선 검토합니다. 빌드 feature 및 모델 경로를 먼저 확인하고, 모델 SHA·출력 SHA가 달라지면 성능 향상과 무관하게 반려합니다.

### IMP-S07 · 장별 출력 flush 횟수와 중단 후 복구 계약을 함께 측정 (P2 계측)

- **확인 근거:** [src/translation/output/chapters.rs L39-L65](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/translation/output/chapters.rs#L39-L65)는 제목, 비어 있지 않은 번역 줄, 화 완료 시 출력 flush를 호출합니다. 같은 파일의 실패 주입 테스트는 완료된 원문 prefix를 유지하는 계약을 검사합니다.
- **개선 절차:** HDD/SSD와 장문 100~1,000행에서 flush 호출 수, 번역 모델 추론 시간, 실제 쓰기 지연을 따로 계측합니다. 파일 쓰기가 유의미한 병목으로 입증된 경우에만 BufWriter 용량·화 단위 flush 정책의 제한적 실험을 검토하며, 진행 중 번역본 표시 및 장애 직전까지의 완료 prefix 보존 정책이 달라지는지 먼저 승인받습니다.
- **검증/원복:** [src/translation/output/chapters.rs L109-L170](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/translation/output/chapters.rs#L109-L170)의 순서·실패 prefix 테스트를 유지. 모델 출력이 같아도 중단 후 파일 복구 범위가 줄면 반려합니다.

### IMP-S08 · INT8 모델 배포와 재설치의 무결성/오프라인 설치 시험 (P1 운영)

- **확인 근거:** [src/models/integrity.rs L5-L27](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/models/integrity.rs#L5-L27)은 모델 파일 크기와 SHA-256을 검사하며, [release.yml L105-L130, L163-L180](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/.github/workflows/release.yml#L105-L130)은 고정 모델 다운로드/Windows 오프라인 번역 점검 경로를 정의합니다.
- **개선 절차:** 매 릴리스에 소스 커밋, 모델 버전·해시, 정확한 ZIP/체크섬, 깨끗한 Windows 사용자 환경의 설치·오프라인 번역·기존 설치 업그레이드를 매핑한 검증표를 보관합니다. 모델 손상·다운로드 중단·디스크 부족·실행 폴더 변경을 실패 주입 테스트로 유지합니다.
- **합격/원복:** 빠른 모델 준비 표시나 다운로드 캐시가 실제 SHA 검사·복원 과정을 우회해서는 안 됩니다. 재설치 후 잘못된 모델이 정상으로 표시되면 릴리스를 중단합니다.

## 5.8 INT8 LLM 추론·토큰 생성의 순수 추가 성능 여력 — 2단계 반복 재검토 (2026-11-11)

### 기존 실측 성과 — 미래 추가 속도와 절대 혼동 금지

[docs/gate70-benchmark.json](https://github.com/P-JSL/Syousetu_Translate/blob/73e01716de6bfcfc5406b68a2753f96fd968314b/docs/gate70-benchmark.json)의 2026-11-10 기록: 동일 Linux AMD EPYC 9V45(4 CPU quota)·동일 고정 HTML/모델에서 이전 reference 대비 35화 **163.11→48.54초, 3.36배**, 24화 **263.90→70.69초, 3.73배**(실행시간 70.24%·73.21% 절감). 이는 **이미 달성된 과거 개선치**, 6개 문서 내 어느 추가 후보의 예상 개선치도 아닙니다. 당시 peak RSS는 약 0.8GiB reference 대비 약 1.1–1.3GiB로 증가하는 trade-off가 있었습니다. [docs/int8-performance.md](https://github.com/P-JSL/Syousetu_Translate/blob/73e01716de6bfcfc5406b68a2753f96fd968314b/docs/int8-performance.md)의 INT8 354,200,608B vs FP32 1,330,973,772B도 **이미 배포에 반영된 가중치 용량 축소**입니다.

### 신규 CPU 추론 가속 후보

| 우선 | 실제 코드 | 구체적 개선·수학적 차이 | 한계 |
|---|---|---|---|
| 1 | [model/layers/batch.rs L59-L69](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/layers/batch.rs#L59-L69) | self-attention 캐시를 매 step Tensor::cat으로 재복사. 사전할당/chunk append 구조로 **KV 기록용 누적 이동 O(T²)→O(T)** 가능성 연구 | attention 계산 그 자체 O(T²), quantized projection, Tensor stride 변환은 여전히 존재. 전체 배수 미측정 |
| 2 | [runner/generation.rs L58-L64](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/generation.rs#L58-L64), [runner/batched/mod.rs L144-L149](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/batched/mod.rs#L144-L149) | 반복 trigram 금지 토큰을 매 생성 step 재스캔하는 대신 **append 증분 해시**로 갱신 | 전체 보조 비용 O(T²)→기대 O(T), 동일 token ID/banned 집합 유지 필요 |
| 3 | [model/ops/attention.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/model/ops/attention.rs) | 기존 fused CPU gemm·thread-local scratch가 있어 단순 융합 중복 금지; cache misses/메모리 traffic이 높을 때만 tile/layout 시험 | GPU FlashAttention 발표 배수 직접 전이 금지 |
| 4 | [offline/scheduler.rs L5-L56](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/scheduler.rs#L5-L56) | 짧은 작업의 serial/parallel 2회 probe cold-start 비용 분리; 길이 분포 기반 early decision 실험 | 잘못된 가속 추정으로 긴 작업 손해를 볼 수 있음 |
| 5 | [runner/batched/mod.rs](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/src/offline/runner/batched/mod.rs) | 4 CPU quota에서 batch/context 1·2·4·8·16 별 tokens/s·p95·peak RSS 비교 | 논리 컨텍스트 128 ≠ CPU 128코어, p95 악화 가능 |
| 6 | [Cargo.toml](https://github.com/P-JSL/Syousetu_Translate/blob/612f743d9aeec166a039d865e332d58fb92b5d1b/Cargo.toml)의 thin LTO / 8 codegen-units | fat LTO / codegen-units=1 / target-cpu=native를 **별도 바이너리에서만** 비교 | native 빌드는 범용 Windows CPU 실행 호환성 상실 가능 |

문헌 비교: [ORCA OSDI22](https://www.usenix.org/conference/osdi22/presentation/yu)의 iteration scheduling/selective batching은 **이미 이 코드에 존재**; [FlashAttention NeurIPS22](https://arxiv.org/abs/2205.14135)는 GPU HBM↔SRAM IO 최적화 원리만 참조; [PagedAttention SOSP23](https://doi.org/10.1145/3600006.3613165)의 대형 LLM 서빙 2–4배 수치는 로컬 소수 컨텍스트 CPU의 예상치가 아닙니다. 모델 pruning/다른 tokenizer/speculative token은 동일 출력 보장 없이 '성능 유지형'으로 적용 금지.

**실험:** 동일 모델 SHA와 동일 35/24화 코퍼스로 새 current HEAD baseline을 만든 뒤 examples/benchmark_offline.rs·examples/benchmark_projection.rs, inference-profile 기능으로 decoder/attention/FFN/matmul, ms/token, tokens/s, first-token ms, RSS를 분해합니다. 5회 이상 측정하여 완성 텍스트 SHA와 토큰 ID가 같아야 합니다. **현행 대비 추가 추론/전체 실행 향상률: 미측정**.

### 추가 향상폭 산출 원칙

동일 기능·동일 입력 기준 전체 시간 T에서 대상 단계 비율 f, 그 단계만 r배 가속할 때 **전체 가속 S=1/((1−f)+f/r)**. 예시 f=10%/r=4이면 전체 **1.081배**, f=50%/r=4이면 **1.600배**, f=80%/r=4이면 **2.500배**. 모두 **가정 시나리오**이며 이 저장소의 측정값·목표치가 아닙니다. 어떤 단계만 무한히 가속해도 전체 최대 가속은 1/(1−f). CPU/메모리/네트워크를 여러 곳 개선해도 국소 배수를 곱해 전체 성능이라고 계산하지 않습니다.

각 후보는 동일 커밋/CPU/메모리/OS, 동일 입력 SHA, warm/cold, 5회 이상 반복, wall-time p50/p95, CPU 시간, peak RSS, 전송 바이트, 처리량, 결과 SHA를 분리해 확인해야 합니다. **실측하지 않은 추가 개선률은 '산출 불가'**입니다.

### 검증한 학술·수학 참고문헌과 경계

- Amdahl (1967), https://doi.org/10.1145/1465482.1465560 — 병렬·부분 최적화의 전체 상한.
- Williams, Waterman, Patterson (2009), https://zenodo.org/records/1236156 — Roofline의 계산/메모리 대역폭 한계.
- Little (1961), https://doi.org/10.1287/opre.9.3.383 — 안정 상태의 동시 작업수 L=λW, 무제한 동시 실행은 지연과 메모리를 악화시킬 수 있음.
- Dean, Barroso (2013), https://research.google/pubs/the-tail-at-scale/ — 처리 평균보다 p95/p99 tail 지연이 악화될 수 있음.
- Google Benchmark 사용자 지침, https://google.github.io/benchmark/user_guide.html — warmup, 독립 반복 측정, 중앙값과 환경 잡음.

**주의:** 문헌의 다른 CPU/GPU/서비스에서 얻은 속도 배수를 이 저장소에 곧바로 적용하지 않습니다.

### 100회 + 100회 반복 검산 범위의 투명성

1차 **100회**는 각 라운드에서 이 보고서의 코드 근거·연산 복잡도·추가 성능 후보·향상률 불확실성 및 서로 다른 workload/병렬도 가정을 다시 대조한 **정적·수치 검산**입니다. 2차 **100회**는 인용 논문·Amdahl, Roofline, Little 모델을 각기 다른 부하·시간비율에서 교차 확인하는 **문헌-수학 정합성 검산**입니다. 두 검산은 각 보고서에 모두 적용했습니다. 단, **200회 독립적인 전 소스 정독이나 200회 실기 벤치마크를 수행한 의미는 아니며**, 정적 계산과 모델 검산만으로 실제 달성 성능을 확정할 수 없습니다.

## 5.9 성능·속도 모델 1,000회 + 논문·수학 검산 1,000회 실행 기록 (2026-11-12)

**실행 방식과 횟수:** 2026-11-12(KST) 로컬 Python 3.13.5에서 실제로 실행한 결정적 합성 입력 기반 자동 검토입니다. **1차 1,000회 × 매 회 5개 서로 다른 항목 = 5,000건, 2차 1,000회 × 매 회 5개 항목 = 5,000건, 해당 저장소 총 2,000회/10,000건이며 실패는 0건**입니다. 6개 저장소 합계 12,000회/60,000건을 기록했습니다. 단순히 1,000개 개선 후보를 만들거나 문장 하나를 1,000번 출력한 것이 아닙니다. 각 라운드는 커밋/저장소/단계/라운드/항목에 의해 시드가 달라집니다.

**범위의 엄격한 한계:** 이것은 **수학 모델·합성 데이터의 파라미터화된 자동 검산 1,000회씩**이며, 실제 저장소 코드를 각 1,000회씩 빌드/실행한 것도, 실모델/실제 브라우저/디스크에서 1,000번 벤치마크한 것도, 논문 본문을 1,000번 정독한 것도 아닙니다. 주요 기존 소스 4개는 별도 GitHub 조회로 내용 식별자와 Git blob SHA를 확인했습니다. 수학 검산 통과만으로 실서비스 버그가 없거나 실측 가속이 달성된다고 주장할 수 없습니다.

**재현 증거:** 검산 스크립트 `performance_review_1000x2_2026-11-12.py` SHA-256 `6e272f565e2bb0741487baf11c19a7b02d1df3158a70926819eb3aeb6c5dbaa8`, 전체 결과 `performance_review_1000x2_results_2026-11-12.json` SHA-256 `1bc85836fca3c8c74c2855d81d12dd4e6412732dc1de49484b9a63803afaf09d`. 두 파일은 이번 분석 작업 산출물이며 **저장소에 소스 코드로 커밋하지 않았습니다**. 결과 JSON에는 각 검증 항목의 1,000건 시나리오 SHA-256과 첫/마지막 시드·측정이 아닌 계산 값이 포함됩니다.

**1차 검산 지문:** `87e5eae44c656d8369041ced05d559e3b85c0e9f35b34cbc625da300ce5d9687`  
**2차 검산 지문:** `a69898cda3b9a229a16ccc1d77475577d2295ae107890b05bcc20f20042e7464`  
(각 지문은 결과 JSON에서 **항목명 사전순으로 정렬된 5개 개별 시나리오 SHA-256 문자열을 `|`로 연결한 값**의 SHA-256입니다.)

### 2차 학술·수학 검산 구성

| 범주 | 매 회 확인하는 핵심 불변식 | 결과 |
|---|---|---|
| Amdahl 상한 | 부분 개선비율 f와 단독 가속 r 변화에 대해 전체 S = 1/(1-f+f/r)의 상한 및 단조성 | **1,000/1,000 통과** |
| Roofline 대역폭 상한 | FLOP/s ≤ min(계산 피크, 산술집약도×메모리 대역폭), 단위와 시간 하한 | **1,000/1,000 통과** |
| Little의 법칙 | 안정된 도착률 λ<서비스율 μ에서 L=λW 및 동시성 증설의 대기 비용 | **1,000/1,000 통과** |
| 꼬리 지연 | p50/p95/최대 지연을 분리해 평균 개선과 p95 개선을 동일시하지 않음 | **1,000/1,000 통과** |
| 저장소 특화 학술 모델 | 저장소별 통계오차·KV 복사·네트워크 임계 경로·I/O 하한·외부정렬·HTTP 파이프라인 | **1,000/1,000 통과** |

[원 논문: Amdahl](https://doi.org/10.1145/1465482.1465560), [Roofline](https://doi.org/10.1145/1498765.1498785), [Little](https://doi.org/10.1287/opre.9.3.383), [The Tail at Scale](https://research.google/pubs/the-tail-at-scale/), [Google Benchmark 반복 측정](https://github.com/google/benchmark/blob/main/docs/user_guide.md). 이 검산은 문헌의 공식을 참고한 모델 정합성 검증으로, 저자들의 실험을 재현하지 않았습니다.

### 현행 전체 속도 향상 배수 — 검산으로 확정할 수 있는 것과 없는 것

Amdahl의 법칙 `S=1/(1-f+f/r)`에서 **전체 시간 중 대상 hot path의 기존 비율 f**가 아직 프로파일링되지 않았으므로 **현행 대비 추가 가속 배수는 미측정/확정 불가**입니다. 예를 들어 f=10%, r=4면 전체 1.081배(시간 7.5% 절약), f=50%, r=4면 1.600배(37.5%), f=80%, r=4면 2.500배(60%)라는 **가정별 이론값**만 계산됩니다. 임의의 여러 최적화 배수를 곱하거나 과거 벤치마크 개선치와 합산하지 않습니다. 실행 최적화 변경 시 동일 자료·하드웨어에서 cold/warm 각각 5회 이상, wall time p50/p95, CPU 시간, peak RSS/heap, I/O, 결과 무결성 검증이 선행돼야 합니다.

### 1차 저장소 특화 성능 검산 구성

| 검산 항목 | 조건별 확인 내용 | 결과 |
|---|---|---|
| `kv_append_vs_concat` | Decoder KV 매 토큰 재연결 복사량 ∑t와 한 번 append t 비교 | **1,000/1,000 통과** |
| `trigram_incremental_equivalence` | 임의 토큰열에서 전체 3-gram 검색과 증분 매핑의 banned 집합 일치 | **1,000/1,000 통과** |
| `batch_work_budget` | CPU budget 상한 8과 논리 컨텍스트 수가 다른 점 검증 | **1,000/1,000 통과** |
| `weight_memory_vs_runtime` | FP32/Q8_0 가중치 용량 감소를 추론시간 가속으로 오인하지 않음 | **1,000/1,000 통과** |
| `attention_compute_lower_bound` | KV 복사를 줄여도 causal attention 쌍 수가 T(T+1)/2임을 검증 | **1,000/1,000 통과** |

### 원본 소스 근거 재확인 (총 4개)

| 파일 (분석 기준 커밋 링크) | Git blob SHA-1 |
|---|---|
| [`src/offline/model/layers/batch.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5628fa40fae4cd0c80ed319375ca1b6d953d4fc2/src/offline/model/layers/batch.rs) | `fc7e7a0dd947bd9fd9ad19f6175c52f6cd021e44` |
| [`src/offline/runner/generation.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5628fa40fae4cd0c80ed319375ca1b6d953d4fc2/src/offline/runner/generation.rs) | `8b9070adb40613e0348daebdbcd18c7b9c00e7c8` |
| [`src/offline/scheduler.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5628fa40fae4cd0c80ed319375ca1b6d953d4fc2/src/offline/scheduler.rs) | `8bc05c87e65c00dbe8462d5d929ada03da39fae8` |
| [`src/offline/model/ops/attention.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5628fa40fae4cd0c80ed319375ca1b6d953d4fc2/src/offline/model/ops/attention.rs) | `6c36c7352981b5da0017beaef221f355280d48fd` |

### 검산을 토대로 유지·추가할 실제 성능 개선 우선순위

**우선순위 1:** `batch.rs`의 self-KV `Tensor::cat`와 `generation.rs`의 반복 3-gram 재계산을 각각 독립적으로 프로파일링합니다. 캐시 복사와 trigram 확인의 보조 비용을 O(T²)에서 O(T)로 줄이는 **알고리즘 여지**는 존재하나 Transformer attention의 점수 계산 자체는 남습니다. **우선순위 2:** batch=1/2/4/8과 `CANDLE_NUM_THREADS=1`에서 tokens/s, p95, peak RSS를 비교하고 모델·tokenizer·최종 TXT SHA 동등성을 확인합니다. **우선순위 3:** thin/fat LTO의 배포 CPU 호환성을 분리 평가합니다. 과거 35화 3.36배/24화 3.73배는 이미 달성된 기존 비교 기록이며 미래 추가 개선으로 합산하지 않습니다.

**연구 적용 범위:** [ORCA](https://www.usenix.org/conference/osdi22/presentation/yu)의 iteration-level scheduling은 이미 일부 구현됨. [FlashAttention](https://proceedings.neurips.cc/paper/2022/hash/67d57c32e20fd0a7a302cb81d36e40d5-Abstract-Conference.html)은 GPU IO-aware 설계, [PagedAttention](https://doi.org/10.1145/3600006.3613165)은 대형 모델 서빙의 KV 메모리 효율 사례이므로 해당 논문의 실측 배수를 CPU 단일 사용자의 SMaLL-100에 전이하지 않습니다.

---

## 5.10 DIVERSE PERFORMANCE REVIEW — 1,000 feasibility + 1,000 cross-disciplinary evaluations (2026-11-12)

### Scope and how the 1,000 rounds were actually run

This is a **new method-diversity audit**, not 1,000 new optimizations. A reproducible Python scenario engine evaluated **10 distinct code-anchored alternatives × 10 workloads × 10 independent constraints = 1,000 feasibility cases**, and a second **10 alternatives × 10 workloads × 10 different research/theory lenses = 1,000 cross-disciplinary cases**, for **2,000 evaluated tuples per repository**. Across six repositories: 12,000 evaluated tuples. Each phase used distinct tuple keys, regenerated a stable SHA-256 and passed uniqueness/assertion checks; repeating the script gave the same summary SHA-256. `run_diverse_1000.py` SHA-256 `caecadb0696142bca8de43e1ae440152015459e8e90e8c418e9909c8a1b82de9`, `summary_6repos.json` SHA-256 `23e3ab0a3a456c954dc7f42a0df115dd9ad26fb81b6aa73c4b74850121fd98d0`.

Crucial limitation: these are **automatic engineering-model evaluations over synthetic scenario parameters**, not 2,000 independent human deep reviews, 2,000 actual code executions, 2,000 program benchmarks, or independent proofs that the alternatives will be faster. The linked existing files were checked against the repository tree. **No candidate speedup is measured or guaranteed.** A "prototype" label only means a plausible experiment subject to correctness gates.

10 workload families include cold start, warm small data, medium, million-item, repeated operations, high RTT, memory pressure, UI churn, slow disk and oversubscribed workloads. The 10 feasibility dimensions are exact-output parity, no new dependency, cross-platform support, bounded memory, tail latency, cold start, cancellation, crash recovery, throughput and I/O overhead.

### Ten **different** research lenses (not another Amdahl/Roofline/Little loop)

1. [Work stealing / dependency span](https://doi.org/10.1145/324133.324234)
2. [Cache-oblivious memory hierarchy](https://www.fftw.org/~athena/abstracts/abstract12.html)
3. [Bloom negative-prefilter tradeoff](https://doi.org/10.1145/362686.362692)
4. [Roaring compressed bitmap indexes](https://doi.org/10.1002/spe.2402)
5. [Boyer–Moore substring search](https://doi.org/10.1145/359842.359859)
6. [Common random numbers for controlled comparisons](https://doi.org/10.1287/mnsc.41.12.1935)
7. [HTTP strong Range validators / RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html)
8. [Top-Down microarchitectural stalls](https://doi.org/10.1109/ISPASS.2014.6844459)
9. [INP interaction responsiveness](https://web.dev/articles/inp)

We evaluated theoretical **relevance**, not automatic applicability. In particular Bloom (only safe as a negative prefilter), Roaring (posting density), string search and web INP are marked **not-relevant** when a method does not involve those resources. Amortization tests the break-even condition *repeats × saved cost > one-time setup*; work/span bounds parallel tasks; cache models distinguish L1/L2 traffic from total FLOPs; BDP = RTT × goodput informs stream window sizing; paired random numbers reduce **benchmark-estimator variance**, not runtime. These models are not interchangeable or universal optimization formulas.

### Audited outcomes: methodological screening, not runtime pass rates

| Phase (1,000 distinct cases each) | Result distribution | Interpretation |
|---|---|---|
| Feasibility | prototype-after-profiling 214; conditional-prototype 394; guard-required 75; low-priority 214; measure-first 3; reject-contract-change 100; existing-not-additional 0 | A scenario may be a good experiment, **not an implemented optimization** |
| Research lenses | useful 360; indirect 230; not-relevant 410 | Marks applicability of a mathematical viewpoint, **not observed wins** |

Feasibility SHA-256: `0cd45ffd603be2b292b51aa1bced1c7baad92b6b9ed9fc2f52c6d9bce44dc181`. Research-lens SHA-256: `de2cbb1a4ce3209e25b8d6d0dc563ae14d76a93bca3ab6788239adebb88403f7`. These hashes cover the generated 1,000-row canonical JSON arrays for the given repository. Machine-readable details are supplied in the separately delivered evidence ZIP, **not committed to this repository**.

### Ten concrete alternatives, feasibility conditions and counterexamples

| ID | Alternative | Verified existing source anchor | Assessment | Only meaningful when | Rejection / regression gate |
|---|---|---|---|---|---|
| S-ALT-01 | Append-only decoder KV buffer | [`src/offline/model/layers/batch.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/model/layers/batch.rs) | **Prototype after profiling** | Long T when Tensor::cat copies past keys/values every step | Exact Candle shape, strides and token equality |
| S-ALT-02 | Incremental trigram state | [`src/offline/runner/generation.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/runner/generation.rs) | **Prototype after profiling** | Long output with repeated per-token windows(3) scans | Exactly matching banned next-token set |
| S-ALT-03 | Length-bucketed iteration batches | [`src/offline/runner/batched/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/runner/batched/mod.rs) | **Conditional** | Large heterogeneous multi-sentence inputs | Correct source order and correction-memory behavior |
| S-ALT-04 | CPU attention tiling | [`src/offline/model/ops/attention.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/model/ops/attention.rs) | **Conditional; fused kernels already present** | Cache miss/PMU evidence under Q8_0 CPU model | No cross-platform CPU assumption or numerical drift |
| S-ALT-05 | SIMD top-1 choice | [`src/offline/selection/simd.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/selection/simd.rs) | **Prototype after profiling; partial SIMD exists** | Projection select measured against current fused selector | Tie/NaN/eos/banned token parity |
| S-ALT-06 | Duplicate-tokenization cache locality | [`src/offline/cache.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/cache.rs) | **Conditional** | Repeated sentences with unchanged correction state | Never cache across changed glossary/memory |
| S-ALT-07 | Skip auto-probe for tiny job | [`src/offline/scheduler.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/scheduler.rs) | **Prototype after profiling** | Serial/parallel probe overhead > total small-job compute | Long mixed workload regression gate |
| S-ALT-08 | Verified model paging/prefetch | [`src/models/integrity.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/models/integrity.rs) | **Conditional** | Cold model load I/O dominates elapsed | No SHA bypass, compare page faults/RSS and Windows |
| S-ALT-09 | CPU-native release experiment | [`Cargo.toml`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/Cargo.toml) | **Conditional, benchmark-only** | Matching test CPU supports chosen instruction ISA | Cannot ship generic executable requiring AVX-only CPU |
| S-ALT-10 | 4-bit different model weights | [`src/offline/model/config.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/5fac4100e4a41c0cbdae81474fd5fbe01d4281da/src/offline/model/config.rs) | **Reject for same-output acceleration** | Only research as separate quality/compatibility mode | Changed translation and tokenizer/quantization behavior |

### Decisions for the next experimental optimization cycle

**First experiments:** (1) separately count self-KV copied bytes and trigram rescans per generated token; (2) prove exact token sequence for all code paths before running time comparisons; (3) investigate short-job auto-probe overhead and length-bucketed batches. A KV append buffer reduces **copy work** but leaves the attention's O(T²) pair interactions, and a faster argmax is useless if Q8 matrix multiplication dominates. The older 3.36×/3.73× full-work speedups and already shipped Q8_0 model compression are **historical achievements**, not future headroom.

Relevant new research perspective: Cache-oblivious memory access (Frigo), amortized suffix-state updates, work-stealing scheduling (Blumofe–Leiserson), microarchitectural stall analysis (Yasin), experimental paired comparison (Nelson–Matejcik).

**Go/no-go before implementation:** collect source/benchmark SHA, build flags, hardware, workload sizes, current E2E wall-time and step breakdown (including p50/p95), CPU cycles, peak RSS/JS heap, I/O bytes, and strict result parity. Run baseline and candidate in randomized alternating order, minimum five complete runs each; preserve all existing public interfaces, persistence semantics and output contracts. A reduction in operation count or a theoretical upper bound **does not prove any whole-program speedup**.

## 5.11 NON-PERFORMANCE QUALITY / SECURITY / RELIABILITY REVIEW — 1,000 + 1,000 scenario screenings (2026-11-12)

### Review protocol and scientific scope

**Actual screening performed, 2026-11-12:** 10 source-anchored controls × 10 synthetic failure classes × 10 operating contexts = **1,000 phase-I combinations**; then the same 10 controls × 10 distinct assurance methods × 10 quality attributes = **1,000 phase-II combinations**. This is **2,000 evaluated scenarios per repository, 12,000 across six repositories**, with deterministic uniqueness/consistency checks and an identical output digest on the second execution. Each permutation is a **model-based screening row**, not an application test, an independent human code review, a vulnerability finding, or proof of compliance.

Phase I contexts: malformed input, corrupted persistence, denied access, simultaneous edits, cancellation, stale version, network/authentication failure, untrusted content, platform mismatch and partial publication, crossed against Windows/Linux/offline/low-disk/concurrent/fresh install/upgrade/restart/restricted account/untrusted-network environments. Phase II methods: property-based testing, metamorphic testing, state-machine verification, fault injection, STRIDE, NIST SSDF, OWASP ASVS, SLSA provenance, WCAG 2.2, and differential testing. A technique is classified **not applicable** when it has no natural relevance; the same standards are **not** force-fitted to every control. 'Existing' means a targeted protection exists in the linked source, **not** the entire threat is solved; 'proposal' is an experiment, **not** a discovered vulnerability.

Evidence: Python model `review_nonperformance_1000x2.py` SHA-256 `3168cd9f7c938c391ff057280fab9a88fd61b86d42b5fba732d564627d22c5da`, summary JSON SHA-256 `e823ec751b255943238598c3a4289f67482d3d8019d5b8bb3e443425a157131b`. The local evidence package includes all 12,000 JSONL records; the files are not committed to this repository. **No target project code was executed, built or modified by this report revision.**

Research/verification frameworks: [NIST SP 800-218 SSDF](https://csrc.nist.gov/pubs/sp/800/218/final), [OWASP ASVS 5.0](https://asvs.owasp.org/), [SLSA 1.2 provenance](https://slsa.dev/spec/v1.2/provenance), [WCAG 2.2](https://www.w3.org/TR/WCAG22/), [QuickCheck property-based testing (ICFP 2000)](https://doi.org/10.1145/351240.351266), [ISO/IEC 25010:2023 quality model](https://www.iso.org/standard/78176.html). These are requirements/checking approaches, **not certifications held by the programs**.

### Repository-specific results and source-claim status

- Phase I: **100 combinations reference the single confirmed source mismatch; 740 lower-priority; 130 existing-guard exercises; 30 test proposals**. These classifications sum to 1,000 and are scenario labels **not distinct problems**.
- Phase II: **55 recommended trials; 469 context-dependent; 476 not relevant** (1,000 combinations, not compliance certifications).
- Controls: **10**. Existing safeguards are listed to prevent recommending them as missing features. 'Confirmed mismatch' means an observed source/config discrepancy, **not a confirmed runtime exploit**.

| ID | Non-performance contract | Source status | Source anchor | Failure scenario | Verification acceptance criterion |
|---|---|---|---|---|---|
| Q-S01 | Portable Windows download directory | **confirmed mismatch** | [`src/config/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/config/mod.rs) | An unset directory currently falls back to a hard-coded user path | Use current-user known folder with explicit configured path precedence; no overwrite |
| Q-S02 | Model ready display trust level | **existing** | [`src/models/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/models/mod.rs) | Size-only badge versus full SHA loading verification | Badge must not imply full integrity; loading rejects same-size tamper |
| Q-S03 | Damaged correction memory | **existing** | [`src/learning/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/learning/mod.rs) | Corrupt remembered replacements under restart | No automatic overwrite or silent loss of user data |
| Q-S04 | Concurrent memory editors | **existing** | [`src/learning/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/learning/mod.rs) | Two simultaneous GUI writers | Lock, reload and atomic merge retain both accepted edits |
| Q-S05 | Transient CLI job lifecycle | **proposal** | [`src/jobs/process.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/jobs/process.rs) | Crash, SIGTERM, disk-full config and failed spawn | Temp config cleaned or safely diagnosed; no misleading success |
| Q-S06 | Redirect/challenge protection | **existing** | [`src/novel/network.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/novel/network.rs) | Cross-origin redirect, oversized HTML, challenge screen | Explicit rejection; no accidental login/anti-bot bypass |
| Q-S07 | Chapter ordering and partial output | **existing** | [`src/translation/output/chapters.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/translation/output/chapters.rs) | Failure midway through multi-episode export | Completed source prefix retained, chapter order unchanged |
| Q-S08 | Offline model payload verification | **existing** | [`src/models/integrity.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/models/integrity.rs) | Replaced model GGUF and offline extracted distribution | Verify size/SHA before inference; do not trust ready badge |
| Q-S09 | Correction vs training distinction | **proposal** | [`src/learning/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/learning/mod.rs) | User glossary edit, restart, new release | Corrected replacement behavior without claiming model fine-tuning |
| Q-S10 | Keyboard and error recovery | **proposal** | [`src/bin/desktop/novel.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/cae25dc4f0a82055e5b29edc8382ff320e1e2008/src/bin/desktop/novel.rs) | Keyboard-only queue, invalid path, offline failure | Focusable controls, intelligible errors and safe retry |

### Cross-check and replacement ledger: old claims vs current code / other repositories

1. The **Windows default download directory is confirmed non-portable**: `src/config/mod.rs:70-74` contains a fixed user home path. An explicit `download_dir` takes precedence. This is a source defect candidate, **not evidence that every PC fails**.
2. A model-ready **size-only indicator is not SHA-256 verification**. Full model preparation/load performs separate checks; the earlier sections discussing 'ready' versus 'verified' should be interpreted accordingly.
3. The historical 5 initial IMP items have grown to **8 IMP-S items**, separately from these 10 Q-S assurance controls. Model inference/quantization results in performance sections must not be interpreted as translation-quality certification.

### Decisions and follow-on tests (non-performance only)

- **Priority P0:** resolve source-confirmed mismatches or potential data-loss pathways first; preserve existing guardrails rather than replacing them with superficially shorter code. Raise code-only changes in separate reviewed PRs.
- **Priority P1:** fault-injection, property-based and differential tests for cancellation, concurrency, corruption, update rollback and release/source parity; document the exact failure oracle and cleanup behavior.
- **Priority P2:** accessibility, diagnosability, user documentation and source-to-build provenance; run keyboard/screen-reader acceptance where a desktop/web UI exists.
- **No claim of completed implementation:** the model review establishes a *risk-informed test plan*, not completed remediation, runtime security testing or proof of ISO/ASVS/SLSA certification.
