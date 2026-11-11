# 5. Syousetu_Translate — 일본어 웹소설 번역기

**저장소:** [P-JSL/Syousetu_Translate](https://github.com/P-JSL/Syousetu_Translate)  
**소스 분석 기준:** `main` / [`612f743d9aeec166a039d865e332d58fb92b5d1b`](https://github.com/P-JSL/Syousetu_Translate/commit/612f743d9aeec166a039d865e332d58fb92b5d1b)  
**보고서 작성일:** 2026-11-11 (KST)  
**대상:** 개발자 · 운영자 · 릴리스 담당자  
**개선 후보:** 5개 (우선순위·수정 위치·구현·검증·원복 기준 포함)  
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
