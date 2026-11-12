# Syousetu_Translate — Codex 작업용 개선 계획서

> **기준 코드:** [`b8e5cc40a98b`](https://github.com/P-JSL/Syousetu_Translate/tree/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb) · **문서 갱신:** 2026-11-12 (KST)  
> **상태:** 분석 및 작업 설계. 아래 작업은 별도 코드 커밋으로 수행해야 하며 이 문서 수정만으로 해결된 것이 아닙니다.  
> **대상:** 일본어 웹소설 로컬 번역기(Rust/SMaLL-100)

## 1. Codex가 먼저 지킬 작업 규칙

1. 해당 작업 ID 하나를 선택한 뒤 **현재 `main`의 실제 소스·호출자·테스트·설정**을 먼저 읽습니다. 이 문서의 기준 커밋은 분석 출처이지 수정할 때 고정해야 하는 오래된 버전이 아닙니다.
2. 작업 범위와 무관한 파일·포맷·공개 API·동작·스키마·리턴 데이터·CI 게이트를 바꾸지 않습니다. 존재하지 않는 메서드·옵션·경로를 만들지 않습니다.
3. 상태가 **기존 보호 유지** 또는 **실험·조건부**라면 결함으로 단정하거나 바로 리팩터링하지 않습니다. 먼저 재현 테스트 또는 프로파일을 만들고 필요성이 입증된 경우에만 변경합니다.
4. 변경 전 실패 사례를 재현하고, 최소 수정 후 기존 테스트 + 신규 회귀를 실행합니다. 실행하지 못한 Windows·모델·GUI·실제 장비 검증은 **미검증**으로 보고합니다.
5. 작업은 **하나씩** 구현·검증·커밋합니다. 완료 보고에는 변경 파일, 이전/이후 동작, 실행 명령·결과, 남은 위험, 커밋 SHA를 기록합니다. 사용자의 별도 요청 없이 릴리스·배포하거나 데이터를 삭제하지 않습니다.

## 2. 프로젝트 핵심 구조·보존 계약

GUI는 작업과 설정을 준비하고 CLI를 별도 프로세스로 구동합니다. 실제 로컬 번역 모델과 교정 메모리, 회차별 출력·복구, 허용된 네트워크 요청을 구분합니다.

- 기존 사용자가 지정한 다운로드 경로가 항상 기본값보다 우선하며 파일을 무단 이동·덮어쓰지 않음.
- 모델 상태 표시(크기 검사)와 실제 추론 전 전체 SHA 검증을 별개로 취급하며 검증되지 않은 모델로 추론하지 않음.
- 교정 메모리의 잠금→재조회→병합→원자적 저장, 손상된 기억 파일 보존. 교정은 가중치 재학습이 아님.
- 회차 순서와 완료된 원문 prefix 보존, 중도 실패/취소 시 기존 출력 파일의 복구 가능성 보존.
- CLI 작업 큐 설정 스냅샷 유지, 출처가 다른 redirect/challenge 우회 금지, 모델·사전·텍스트의 동일성 확인.

## 3. 현재 확인된 사실과 미해결 조건

- 확인된 미해결 문제: `src/config/mod.rs`에서 미설정 Windows 저장 경로가 특정 사용자 OneDrive 폴더에 고정됨.
- 이미 보호됨: 모델 GUI 상태 크기 검사와 실제 모델 SHA 검사 분리, 교정 기억 잠금/원자적 저장, 네트워크 리다이렉트 제한, 번역 출력 prefix 테스트.
- 실측 참고: 기존 특정 Linux 조건의 35화 약 3.36배·24화 약 3.73배는 과거 개선 결과이며 **추가 예상 배수로 중복 계산하지 않음**.

## 4. 작업 순서표

우선순위 **P0=정합성·데이터 보호**, **P1=검증·실측 필요**, **P2=조건부 최적화·사용성**입니다. 표의 '판정'은 이미 고쳐진 상태를 뜻하지 않습니다.

| ID | 우선 | 작업 | 현재 판정 |
|---|---|---|---|
| S-01 | P0 | Windows 기본 다운로드 위치를 현재 사용자 기반으로 교체 | 실제 코드에서 확인된 비이식성 |
| S-02 | P1 | 모델 준비 표시와 무결성 문구 구별 | 이미 구현된 보호 경계의 UI 정합성 |
| S-03 | P1 | 교정 기억 동시 편집·파일 손상 복구 검증 | 기존 잠금/원자성 회귀 |
| S-04 | P1 | GUI 작업 큐와 CLI 자식 프로세스 수명주기 | 중단·실패 주입 |
| S-05 | P1 | 원고 다운로드 보안·오류 분류 | 기존 네트워크 제한 회귀 |
| S-06 | P1 | 회차별 출력의 순서·중단 복구 보존 | 기존 prefix 계약 회귀 |
| S-07 | P1 | 실모델 통합 검증과 릴리스 출처 관리 | 고정 모델이 준비된 실행 환경에서만 |
| S-08 | P1 | KV 캐시 중복 복사·배치 비용 프로파일링 | 측정 우선/동작 변경 전 보류 |
| S-09 | P2 | 3-gram 금지 토큰 계산을 증분화 | 독립적 정확성 증명 필요 |
| S-10 | P2 | 짧은 작업의 자동 실행 정책과 배치 구성 튜닝 | 기존 정책 우선 확인 |
| S-11 | P2 | GUI 접근성·오류 메시지 정리 | 시각·키보드 테스트 필요 |

## 5. Codex 실행용 상세 작업지시서

### S-01 · Windows 기본 다운로드 위치를 현재 사용자 기반으로 교체 (P0)

- **판정:** 실제 코드에서 확인된 비이식성
- **수정·확인 파일:** [`src/config/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/config/mod.rs), [`src/config/tests.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/config/tests.rs)
- **구현 지시:** 현재 `download_directory()`의 미설정 Windows 반환값에 고정 사용자 경로가 존재합니다. 사용자가 지정한 `download_dir` 우선순위를 유지하고 Windows 현재 사용자에게 유효한 문서/다운로드 위치로 기본값을 구성합니다. 변경 전·후 기본값/빈 문자열/비Windows/사용자 지정 설정을 테스트합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 다른 Windows 계정에서도 고정 사용자명이 없고 이미 저장된 사용자 경로가 보존됨
- **중단·원복:** 저장 위치를 임의 변경하거나 기존 파일 이동 금지
- **기존 보고서 연결:** Q-S01

### S-02 · 모델 준비 표시와 무결성 문구 구별 (P1)

- **판정:** 이미 구현된 보호 경계의 UI 정합성
- **수정·확인 파일:** [`src/models/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/models/mod.rs), [`src/models/integrity.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/models/integrity.rs)
- **구현 지시:** 모델 상태가 크기 검사를 뜻하는지 전체 SHA 확인을 뜻하는지 UI 문구를 확인·수정합니다. 동일 크기 변조 파일을 상태 표시 후 실제 추론에서 차단하는 테스트를 유지합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 상태 문구가 검사 수준을 과장하지 않으며 추론 준비 시 전체 SHA 검사가 필수
- **중단·원복:** 상태 표시 최적화로 SHA 검사를 생략하지 않음
- **기존 보고서 연결:** Q-S02, Q-S08, IMP-S08

### S-03 · 교정 기억 동시 편집·파일 손상 복구 검증 (P1)

- **판정:** 기존 잠금/원자성 회귀
- **수정·확인 파일:** [`src/learning/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/learning/mod.rs), [`src/offline/cache.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/cache.rs)
- **구현 지시:** 두 GUI 인스턴스의 동시 수정, 손상 압축파일, 비정상 종료/재시작을 시험합니다. 업데이트·삭제 후 교정 캐시가 오래된 용어를 재사용하지 않는지 확인합니다. 핫패스 최적화는 실패/복구 계약 보존 후 고려합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 교정 내용 유실·비의도 초기화·동일 키 충돌이 없고 cache eviction이 정확함
- **중단·원복:** 잠금·원자적 저장을 제거하는 성능 변경 금지
- **기존 보고서 연결:** Q-S03, Q-S04, Q-S09, IMP-S03

### S-04 · GUI 작업 큐와 CLI 자식 프로세스 수명주기 (P1)

- **판정:** 중단·실패 주입
- **수정·확인 파일:** [`src/jobs/process.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/jobs/process.rs), [`src/jobs/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/jobs/mod.rs), [`tests/desktop_queue.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/tests/desktop_queue.rs)
- **구현 지시:** 설정 스냅샷·임시 `config.properties`·프로세스 종료·재시작·취소·CLI 부재·권한 거부를 분리해 테스트합니다. 요청한 작업 하나만 실패로 표시하고 다른 작업의 큐·교정 상태는 유지합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 실패 원인이 UI에 명확히 표시되고 임시 설정 잔재·고아 프로세스가 남지 않음
- **중단·원복:** 다른 작업까지 취소하거나 임시 경로를 전역 고정하지 않음
- **기존 보고서 연결:** Q-S05

### S-05 · 원고 다운로드 보안·오류 분류 (P1)

- **판정:** 기존 네트워크 제한 회귀
- **수정·확인 파일:** [`src/novel/network.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/novel/network.rs)
- **구현 지시:** 도메인 간 리다이렉트, 챌린지 페이지, 잘못된 인코딩/큰 HTML, timeout/연속 재시도를 테스트합니다. 사이트 정책 또는 로그인 우회를 추가하지 않으며 retry 횟수와 오류 메시지에서 URL/인증 데이터가 새지 않게 확인합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 차단된 응답은 명시적 실패하고 원고로 조용히 저장되지 않음
- **중단·원복:** 안티봇·접근 제어 우회 또는 무제한 재시도 금지
- **기존 보고서 연결:** Q-S06

### S-06 · 회차별 출력의 순서·중단 복구 보존 (P1)

- **판정:** 기존 prefix 계약 회귀
- **수정·확인 파일:** [`src/translation/output/chapters.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/translation/output/chapters.rs), [`tests/production_cli.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/tests/production_cli.rs)
- **구현 지시:** 1화 성공→2화 중단, 부분 번역 실패, 빈 회차, 회차 타이틀, 재개 시나리오를 검증합니다. 완료 prefix만 유지하는 현재 계약과 flush 의미가 달라지지 않도록 검사합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 정상 회차 순서, 중복/누락 0건, 완료 prefix 복구 보존
- **중단·원복:** 버퍼링 최적화로 장애 시 복구 가능한 범위를 줄이지 않음
- **기존 보고서 연결:** IMP-S07, Q-S07

### S-07 · 실모델 통합 검증과 릴리스 출처 관리 (P1)

- **판정:** 고정 모델이 준비된 실행 환경에서만
- **수정·확인 파일:** [`src/models/integrity.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/models/integrity.rs), [`.github/workflows/release.yml`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/.github/workflows/release.yml), [`tests/offline_model.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/tests/offline_model.rs)
- **구현 지시:** 일반 CI와 실제 SMaLL-100 모델이 필요한 ignored 통합 시험을 분리합니다. 모델 SHA와 코퍼스/출력 해시, 클린 Windows 오프라인 설치·재설치, 누락/손상 모델을 release 검증표에 연계합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 정식 배포 경로에서 모델 무결성·오프라인 번역·업그레이드 검증 결과가 추적됨
- **중단·원복:** 모델 없이 진행한 테스트를 실모델 검증 완료라고 표시하지 않음
- **기존 보고서 연결:** IMP-S06, IMP-S08

### S-08 · KV 캐시 중복 복사·배치 비용 프로파일링 (P1)

- **판정:** 측정 우선/동작 변경 전 보류
- **수정·확인 파일:** [`src/offline/model/layers/batch.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/model/layers/batch.rs), [`src/offline/model/ops/attention.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/model/ops/attention.rs), [`examples/benchmark_offline.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/examples/benchmark_offline.rs)
- **구현 지시:** 디코더가 토큰마다 `Tensor::cat`을 호출하는 부분의 누적 복사 바이트와 attention/FFN/행렬 시간 비중을 먼저 측정합니다. 유의미할 때만 사전할당·청크형 캐시를 별도 프로토타입으로 비교합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 모델 SHA·생성 토큰열·교정 결과 동등, 실제 전체 추론 시간과 RSS 측정값 개선
- **중단·원복:** Candle 텐서 모양/stride가 다르거나 출력 토큰이 바뀌면 즉시 반려
- **기존 보고서 연결:** IMP-S01, IMP-S05

### S-09 · 3-gram 금지 토큰 계산을 증분화 (P2)

- **판정:** 독립적 정확성 증명 필요
- **수정·확인 파일:** [`src/offline/runner/generation.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/runner/generation.rs), [`src/offline/runner/batched/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/runner/batched/mod.rs)
- **구현 지시:** 현재 `windows(3)` 전체 스캔을 길이별 상태로 대체하는 helper를 시제품으로 만들고, 한 토큰 추가/문장 종료/배치 취소·복구를 포함해 기존 함수의 금지 토큰 집합과 매 스텝 비교합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 모든 골든 토큰열의 금지 집합·출력 동일, 긴 입력에서만 실측 가속 확인
- **중단·원복:** 해시 충돌·순서·문장 경계가 달라지면 유지하지 않음
- **기존 보고서 연결:** IMP-S02

### S-10 · 짧은 작업의 자동 실행 정책과 배치 구성 튜닝 (P2)

- **판정:** 기존 정책 우선 확인
- **수정·확인 파일:** [`src/offline/scheduler.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/scheduler.rs), [`src/offline/runner/batched/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/runner/batched/mod.rs), [`src/offline/profiling.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/profiling.rs)
- **구현 지시:** 짧은 문장의 serial/parallel probe와 batch 크기 1/2/4/8을 같은 CPU quota·골든 번역으로 비교합니다. 기존 선택 방식이 이미 최적인 경우 변경하지 않습니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 추론 전체 wall-time/첫 토큰 시간/RSS와 결과·순서가 함께 검증됨
- **중단·원복:** 단일 사례에서만 유리하거나 첫 실행 지연이 악화되면 원복
- **기존 보고서 연결:** IMP-S04, IMP-S05

### S-11 · GUI 접근성·오류 메시지 정리 (P2)

- **판정:** 시각·키보드 테스트 필요
- **수정·확인 파일:** [`src/bin/desktop/novel.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/bin/desktop/novel.rs)
- **구현 지시:** 목차→범위→저장경로→번역→취소·재시도 흐름을 키보드로 점검하고 오류 코드·원인·대응 방법을 분리합니다. 현재 UI framework 구조·기존 핸들러를 유지합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 오류가 복구 가능한 안내로 표시되고 키보드 조작 경로가 끊기지 않음
- **중단·원복:** 임의 GUI API 교체 및 동작 변경 금지
- **기존 보고서 연결:** Q-S10

## 6. 개발·CI 검증 순서

```bash
cargo test --locked --release --features gui --lib --bins --tests
cargo clippy --locked --features gui --all-targets -- -D warnings
cargo build --locked --release --features gui --bins
cargo test --locked --no-default-features --features legacy --all-targets
python -m unittest discover -s scripts -p 'test_*.py'
```

위 명령은 저장소에서 확인한 빌드·워크플로와 테스트를 기준으로 작성했습니다. Linux GUI 의존성, Windows 실행 환경, 실모델·실데이터 등 필요한 조건이 없으면 실패 원인을 분리하고 **실행 성공으로 보고하지 마세요**. 시간·메모리·처리량은 동일 머신과 입력에서 기준선/변경본을 각각 반복 측정한 경우에만 비교합니다.

## 7. 이력 통합·분류 원칙

- 기존 문서의 `IMP-*` 항목, 별도의 `Q-*` 품질 검토 및 성능 대안표는 위 작업 ID에 연결해 **중복된 과제를 하나로 합쳤습니다**. 위에 별도 표기하지 않은 과거 제안은 현행 구현에 이미 포함됐거나 근거·계측이 부족한 조건부 아이디어로 간주합니다.
- 이전의 **100회/1,000회 반복 검토**는 합성 시나리오의 모델 검산으로, 실제 소프트웨어를 같은 횟수만큼 실행·프로파일링했다는 뜻이 아닙니다. 산출물은 별도 검산 패키지로 보존하고 여기에는 실행 작업에 필요한 결론만 반영합니다.
- `P0`는 위험의 우선순위이지 이미 장애가 발생했다는 판정이 아닙니다. 보안 이론·논문의 다른 시스템 개선 배수를 이 프로젝트의 성과로 옮기지 않습니다.
- **최종 완료 판정:** 확인된 불일치는 실제 소스 수정 및 테스트가 통과해야만 '해결'로 변경합니다. 이 문서는 **Codex 작업 지침**이며 구현 완료 증거가 아닙니다.
