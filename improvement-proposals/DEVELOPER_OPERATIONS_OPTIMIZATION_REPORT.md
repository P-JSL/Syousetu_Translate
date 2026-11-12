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
- **근거 및 관찰 사실:** `src/config/mod.rs` 64~75행을 확인했으며, Windows 미설정 경로가 `C:\Users\rabbit\OneDrive\문서`로 고정된 것은 확인된 소스 문제입니다. 명시된 사용자 설정은 `filter`/`map`을 통해 우선 적용됩니다.
- **적용 원리·수식 또는 불변식:** 경로 우선순위 `유효한 사용자 설정 > 운영체제 현재 사용자 기본폴더 > 안전한 오류`; 불변식 `설정된 기존 경로는 자동 이동/덮어쓰기 금지`.
- **공식 문서·논문 출처:** [Microsoft Windows Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid) — 고정 계정명을 만들지 말고 현재 사용자 프로필에서 경로를 결정합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 다른 Windows 계정에서도 고정 사용자명이 없고 이미 저장된 사용자 경로가 보존됨
- **중단·원복:** 저장 위치를 임의 변경하거나 기존 파일 이동 금지
- **기존 보고서 연결:** Q-S01

### S-02 · 모델 준비 표시와 무결성 문구 구별 (P1)

- **판정:** 이미 구현된 보호 경계의 UI 정합성
- **수정·확인 파일:** [`src/models/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/models/mod.rs), [`src/models/integrity.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/models/integrity.rs)
- **구현 지시:** 모델 상태가 크기 검사를 뜻하는지 전체 SHA 확인을 뜻하는지 UI 문구를 확인·수정합니다. 동일 크기 변조 파일을 상태 표시 후 실제 추론에서 차단하는 테스트를 유지합니다.
- **근거 및 관찰 사실:** `src/models/mod.rs`의 표시용 크기 검사와 `src/models/integrity.rs`의 전체 해시 검증은 목적이 다릅니다. 같은 크기의 손상 파일은 표시가 준비됨으로 보일 수 있으나 모델 로드에서 별도 검사됩니다.
- **적용 원리·수식 또는 불변식:** 모델 사용 조건 `ready_for_inference ⇒ expected_size==actual_size ∧ expected_sha256==actual_sha256`; UI의 `size_ok`만으로 `sha_ok`를 추론하지 않습니다.
- **공식 문서·논문 출처:** [NIST FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final), [Rust 표준 Hash 문서](https://doc.rust-lang.org/std/hash/index.html) — 일반 해시 테이블 해시와 암호학적 SHA-256을 구별합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 상태 문구가 검사 수준을 과장하지 않으며 추론 준비 시 전체 SHA 검사가 필수
- **중단·원복:** 상태 표시 최적화로 SHA 검사를 생략하지 않음
- **기존 보고서 연결:** Q-S02, Q-S08, IMP-S08

### S-03 · 교정 기억 동시 편집·파일 손상 복구 검증 (P1)

- **판정:** 기존 잠금/원자성 회귀
- **수정·확인 파일:** [`src/learning/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/learning/mod.rs), [`src/offline/cache.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/cache.rs)
- **구현 지시:** 두 GUI 인스턴스의 동시 수정, 손상 압축파일, 비정상 종료/재시작을 시험합니다. 업데이트·삭제 후 교정 캐시가 오래된 용어를 재사용하지 않는지 확인합니다. 핫패스 최적화는 실패/복구 계약 보존 후 고려합니다.
- **근거 및 관찰 사실:** `src/learning/mod.rs`의 교정 기억 변경 및 `src/offline/cache.rs`의 캐시를 대상으로 합니다. 기존 잠금/원자적 저장이 존재한다는 선행 분석을 기준으로 신규 결함 여부를 테스트합니다.
- **적용 원리·수식 또는 불변식:** 동시 편집의 직렬화 가능성: 두 업데이트 `U₁,U₂`가 각각 성공했으면 최종 데이터에 승인된 두 결과가 반영되어야 합니다. 원자 교체는 `완료 파일 = 구버전 또는 신버전`, 절반만 쓰인 버전 금지.
- **공식 문서·논문 출처:** [Python os.replace의 원자적 이름 교체 개념](https://docs.python.org/3/library/os.html#os.replace), [NIST SSDF](https://csrc.nist.gov/pubs/sp/800/218/final) — Python API는 Rust 구현을 직접 정의하지 않는 비교 참고입니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 교정 내용 유실·비의도 초기화·동일 키 충돌이 없고 cache eviction이 정확함
- **중단·원복:** 잠금·원자적 저장을 제거하는 성능 변경 금지
- **기존 보고서 연결:** Q-S03, Q-S04, Q-S09, IMP-S03

### S-04 · GUI 작업 큐와 CLI 자식 프로세스 수명주기 (P1)

- **판정:** 중단·실패 주입
- **수정·확인 파일:** [`src/jobs/process.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/jobs/process.rs), [`src/jobs/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/jobs/mod.rs), [`tests/desktop_queue.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/tests/desktop_queue.rs)
- **구현 지시:** 설정 스냅샷·임시 `config.properties`·프로세스 종료·재시작·취소·CLI 부재·권한 거부를 분리해 테스트합니다. 요청한 작업 하나만 실패로 표시하고 다른 작업의 큐·교정 상태는 유지합니다.
- **근거 및 관찰 사실:** `src/jobs/process.rs`와 `tests/desktop_queue.rs`에서 GUI와 CLI 프로세스의 생명주기를 분리해 검토합니다. 종료/실패 시 임시 설정 및 자식 프로세스 정리 여부는 테스트로 확인합니다.
- **적용 원리·수식 또는 불변식:** 작업 상태 전이 `queued → running → (done | failed | cancelled)`. 불변식: 한 작업의 취소가 다른 독립 작업의 terminal 상태를 바꾸지 않음; 성공했을 때만 완료로 게시.
- **공식 문서·논문 출처:** [Rust `std::process::Child`](https://doc.rust-lang.org/std/process/struct.Child.html), [NIST SSDF](https://csrc.nist.gov/pubs/sp/800/218/final) — OS 프로세스 종료와 파일 저장의 정합성은 별도 검증해야 합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 실패 원인이 UI에 명확히 표시되고 임시 설정 잔재·고아 프로세스가 남지 않음
- **중단·원복:** 다른 작업까지 취소하거나 임시 경로를 전역 고정하지 않음
- **기존 보고서 연결:** Q-S05

### S-05 · 원고 다운로드 보안·오류 분류 (P1)

- **판정:** 기존 네트워크 제한 회귀
- **수정·확인 파일:** [`src/novel/network.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/novel/network.rs)
- **구현 지시:** 도메인 간 리다이렉트, 챌린지 페이지, 잘못된 인코딩/큰 HTML, timeout/연속 재시도를 테스트합니다. 사이트 정책 또는 로그인 우회를 추가하지 않으며 retry 횟수와 오류 메시지에서 URL/인증 데이터가 새지 않게 확인합니다.
- **근거 및 관찰 사실:** `src/novel/network.rs`의 HTTP 응답·redirect·사이트별 오류 판정 경계입니다. 챌린지 응답은 성공한 원고가 아니며 무조건 우회 구현을 제안하지 않습니다.
- **적용 원리·수식 또는 불변식:** `성공한 원고 응답 ⇒ 승인된 출처 ∧ 허용 콘텐츠 유형 ∧ 크기·인코딩 검증`. 재시도 횟수 `n ≤ 설정된 상한`, 처리시간 상한은 요청 timeout을 기준으로 확인합니다.
- **공식 문서·논문 출처:** [RFC 9110 HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110.html), [OWASP SSRF 예방](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html) — 로컬 클라이언트의 외부 URL 접근에는 적용 범위를 조정합니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 차단된 응답은 명시적 실패하고 원고로 조용히 저장되지 않음
- **중단·원복:** 안티봇·접근 제어 우회 또는 무제한 재시도 금지
- **기존 보고서 연결:** Q-S06

### S-06 · 회차별 출력의 순서·중단 복구 보존 (P1)

- **판정:** 기존 prefix 계약 회귀
- **수정·확인 파일:** [`src/translation/output/chapters.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/translation/output/chapters.rs), [`tests/production_cli.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/tests/production_cli.rs)
- **구현 지시:** 1화 성공→2화 중단, 부분 번역 실패, 빈 회차, 회차 타이틀, 재개 시나리오를 검증합니다. 완료 prefix만 유지하는 현재 계약과 flush 의미가 달라지지 않도록 검사합니다.
- **근거 및 관찰 사실:** `src/translation/output/chapters.rs`는 회차 순서와 중단 시 완료 prefix를 보존하는 테스트를 갖고 있습니다. 버퍼링이나 병렬 출력은 그 계약을 침해할 수 있습니다.
- **적용 원리·수식 또는 불변식:** 완료 접두 보존 `output_after_failure = concat(chapter_1 ... chapter_k)`(완료된 순서만). `write(chapter_i)` 이전에 `chapter_{i-1}` 출력이 확정됐는지 확인합니다.
- **공식 문서·논문 출처:** [Rust `Write::flush`](https://doc.rust-lang.org/std/io/trait.Write.html#tymethod.flush), [Rust BufWriter](https://doc.rust-lang.org/std/io/struct.BufWriter.html) — flush 반환과 강제 종료 시 데이터 내구성은 별도입니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 정상 회차 순서, 중복/누락 0건, 완료 prefix 복구 보존
- **중단·원복:** 버퍼링 최적화로 장애 시 복구 가능한 범위를 줄이지 않음
- **기존 보고서 연결:** IMP-S07, Q-S07

### S-07 · 실모델 통합 검증과 릴리스 출처 관리 (P1)

- **판정:** 고정 모델이 준비된 실행 환경에서만
- **수정·확인 파일:** [`src/models/integrity.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/models/integrity.rs), [`.github/workflows/release.yml`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/.github/workflows/release.yml), [`tests/offline_model.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/tests/offline_model.rs)
- **구현 지시:** 일반 CI와 실제 SMaLL-100 모델이 필요한 ignored 통합 시험을 분리합니다. 모델 SHA와 코퍼스/출력 해시, 클린 Windows 오프라인 설치·재설치, 누락/손상 모델을 release 검증표에 연계합니다.
- **근거 및 관찰 사실:** `tests/offline_model.rs`의 ignored 모델 통합 시험과 `.github/workflows/release.yml`의 배포 검증 범위가 다릅니다. 모델이 없는 일반 CI 성공을 오프라인 모델 성공으로 표시하지 않습니다.
- **적용 원리·수식 또는 불변식:** 릴리스 추적 관계 `artifact_SHA = hash(출하 아티팩트)`, `model_SHA = hash(동봉 모델)`로 독립 보존합니다. `release_candidate ⇒ 일반 CI 통과 ∧ 별도 모델 시험 통과`는 도입을 검토할 게이트입니다.
- **공식 문서·논문 출처:** [SLSA Provenance](https://slsa.dev/spec/v1.2/provenance), [Cargo test 공식 문서](https://doc.rust-lang.org/cargo/commands/cargo-test.html) — ignored test는 명시해 실행하지 않으면 수행되지 않습니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 정식 배포 경로에서 모델 무결성·오프라인 번역·업그레이드 검증 결과가 추적됨
- **중단·원복:** 모델 없이 진행한 테스트를 실모델 검증 완료라고 표시하지 않음
- **기존 보고서 연결:** IMP-S06, IMP-S08

### S-08 · KV 캐시 중복 복사·배치 비용 프로파일링 (P1)

- **판정:** 측정 우선/동작 변경 전 보류
- **수정·확인 파일:** [`src/offline/model/layers/batch.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/model/layers/batch.rs), [`src/offline/model/ops/attention.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/model/ops/attention.rs), [`examples/benchmark_offline.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/examples/benchmark_offline.rs)
- **구현 지시:** 디코더가 토큰마다 `Tensor::cat`을 호출하는 부분의 누적 복사 바이트와 attention/FFN/행렬 시간 비중을 먼저 측정합니다. 유의미할 때만 사전할당·청크형 캐시를 별도 프로토타입으로 비교합니다.
- **근거 및 관찰 사실:** `src/offline/model/layers/batch.rs` 64행의 두 번 `Tensor::cat()`이 이전 K/V를 복사하는 후보로 확인됐습니다. 실제 CPU 시간 비중은 별도 프로파일링 필요입니다.
- **적용 원리·수식 또는 불변식:** 길이 `T`의 매 step 재연결 복사 요소 수는 대략 `Σ_{t=1}^{T} t = T(T+1)/2 = O(T²)`. 재사용 가능한 append 슬롯은 적어도 단순 기록 `O(T)` 여지가 있으나 attention 계산 `O(T²)`는 여전히 남습니다.
- **공식 문서·논문 출처:** [FlashAttention IO-aware 논문](https://arxiv.org/abs/2205.14135) — GPU 실측 개선배수를 CPU Candle 구현에 그대로 적용하지 않습니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 모델 SHA·생성 토큰열·교정 결과 동등, 실제 전체 추론 시간과 RSS 측정값 개선
- **중단·원복:** Candle 텐서 모양/stride가 다르거나 출력 토큰이 바뀌면 즉시 반려
- **기존 보고서 연결:** IMP-S01, IMP-S05

### S-09 · 3-gram 금지 토큰 계산을 증분화 (P2)

- **판정:** 독립적 정확성 증명 필요
- **수정·확인 파일:** [`src/offline/runner/generation.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/runner/generation.rs), [`src/offline/runner/batched/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/runner/batched/mod.rs)
- **구현 지시:** 현재 `windows(3)` 전체 스캔을 길이별 상태로 대체하는 helper를 시제품으로 만들고, 한 토큰 추가/문장 종료/배치 취소·복구를 포함해 기존 함수의 금지 토큰 집합과 매 스텝 비교합니다.
- **근거 및 관찰 사실:** `src/offline/runner/generation.rs` 101행에서 `.windows(3)` 전 구간 검사가 확인됐습니다. batch 경로와 출력이 정확히 같은지 검증하는 것이 선행됩니다.
- **적용 원리·수식 또는 불변식:** 각 토큰 생성에서 현재 시퀀스 길이만큼 전체 3-gram을 재검색하면 누적 `Σ t = O(T²)`; `(앞의 두 토큰)→금지된 다음 토큰 집합`을 증분 갱신할 때 예상 탐색비용 `O(T)`(평균적 해시 연산 전제).
- **공식 문서·논문 출처:** [Rust slice windows](https://doc.rust-lang.org/std/primitive.slice.html#method.windows), [HashMap](https://doc.rust-lang.org/std/collections/struct.HashMap.html) — 평균 복잡도를 최악 상한으로 오인하지 않습니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 모든 골든 토큰열의 금지 집합·출력 동일, 긴 입력에서만 실측 가속 확인
- **중단·원복:** 해시 충돌·순서·문장 경계가 달라지면 유지하지 않음
- **기존 보고서 연결:** IMP-S02

### S-10 · 짧은 작업의 자동 실행 정책과 배치 구성 튜닝 (P2)

- **판정:** 기존 정책 우선 확인
- **수정·확인 파일:** [`src/offline/scheduler.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/scheduler.rs), [`src/offline/runner/batched/mod.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/runner/batched/mod.rs), [`src/offline/profiling.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/offline/profiling.rs)
- **구현 지시:** 짧은 문장의 serial/parallel probe와 batch 크기 1/2/4/8을 같은 CPU quota·골든 번역으로 비교합니다. 기존 선택 방식이 이미 최적인 경우 변경하지 않습니다.
- **근거 및 관찰 사실:** `src/offline/scheduler.rs`는 serial/parallel 선택을 위한 probe를 수행합니다. probe가 짧은 요청의 병목인지 실측 없이는 확정할 수 없습니다.
- **적용 원리·수식 또는 불변식:** `T_auto = T_probe + min(T_serial,T_batch)`로 단순 분해하되 probe가 본 작업과 자원을 공유하면 정확한 측정식은 달라집니다. 실험 지표는 토큰/s, 첫 토큰 ms, p95, RSS입니다.
- **공식 문서·논문 출처:** [Work-Stealing Theory](https://doi.org/10.1145/324133.324234), [Google Benchmark](https://google.github.io/benchmark/user_guide.html) — 작업 수 증가가 무조건 지연을 낮추지는 않습니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests`
- **완료 조건:** 추론 전체 wall-time/첫 토큰 시간/RSS와 결과·순서가 함께 검증됨
- **중단·원복:** 단일 사례에서만 유리하거나 첫 실행 지연이 악화되면 원복
- **기존 보고서 연결:** IMP-S04, IMP-S05

### S-11 · GUI 접근성·오류 메시지 정리 (P2)

- **판정:** 시각·키보드 테스트 필요
- **수정·확인 파일:** [`src/bin/desktop/novel.rs`](https://github.com/P-JSL/Syousetu_Translate/blob/b8e5cc40a98bce199673ea10b22dce4cfeb6f3eb/src/bin/desktop/novel.rs)
- **구현 지시:** 목차→범위→저장경로→번역→취소·재시도 흐름을 키보드로 점검하고 오류 코드·원인·대응 방법을 분리합니다. 현재 UI framework 구조·기존 핸들러를 유지합니다.
- **근거 및 관찰 사실:** `src/bin/desktop/novel.rs`의 화면·오류 메시지는 실제 GUI에서 keyboard-only/오프라인 오류를 시험해야 합니다. 코드 리뷰만으로 접근성 충족을 확정하지 않습니다.
- **적용 원리·수식 또는 불변식:** 업무 흐름 `URL → 범위 → 출력경로 → 실행 → 취소/재시도`의 각 단계에 키보드 조작 경로 하나 이상, 실패 시 상태·복구 가능 정보가 필요합니다.
- **공식 문서·논문 출처:** [WCAG 2.2 키보드 원칙](https://www.w3.org/WAI/WCAG22/Understanding/keyboard), [WCAG 상태 메시지](https://www.w3.org/WAI/WCAG22/Understanding/status-messages) — 네이티브 GUI에는 해당 취지를 플랫폼에 맞게 적용합니다.
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

## 8. 추가 보안·위험·복구 보완 사항 — 소스 재검증 (2026-11-12)

실제 코드에서 확인한 교체/파일 읽기 경계와 테스트가 필요한 장애 가능성을 구분했습니다. 기존 S-01~S-11의 구현·검증 계약은 유지하며, 취약점 악용이나 데이터 손상 발생을 확인했다고 주장하지 않습니다.

### 8.1 신규 보완 대상

| ID | 우선순위 | 내용 | 확인 상태 |
|---|---|---|---|
| S-12 | P1 | 모델 설치 파일 교체의 기존 정상 모델 보존 | 기존 파일 삭제 후 rename하는 순서 확인; 실제 손상 미재현 |
| S-13 | P1 | 로컬 오프라인 HTML의 읽기 용량·리소스 제한 | 네트워크 16MiB 제한과 로컬 `read_to_string`의 차이를 확인 |
| S-14 | P1 | 출력 AtomicFile의 중간 종료·백업 복구 | 정상 오류 시 복구 구현은 존재, 강제 중단 시나리오는 미검증 |
| S-15 | P2 | 사용자 추가 CA 선택 경로와 TLS 오류 진단 | 설정/환경변수 CA 우선순위 확인; 악용·오설정 발생 미확인 |

### 8.2 Codex 상세 지시서

### S-12 · 모델 설치 파일 교체의 기존 정상 모델 보존 (P1)

- **근거 소스:** [`src/models/download/transport.rs` L1-L8](https://github.com/P-JSL/Syousetu_Translate/blob/1f9bd9671b0a537f745bfccc768e91154f35003a/src/models/download/transport.rs#L1-L8)
- **현재 판정:** 기존 파일 삭제 후 rename하는 순서 확인; 실제 손상 미재현
- **확인된 코드 근거:** `install()`은 기존 모델이 있으면 `remove_file(path)` 후 검증된 임시 모델을 `rename()`합니다. 설치/전송의 전체 SHA 검사는 이미 구현돼 있으므로 삭제 단계의 안전성만 추가 분석합니다.
- **구현·검증 방안:** 완성된 `.part`·기존 정상 모델을 고정하고, 삭제 직후/rename 실패/잠긴 Windows 파일/백신 점유/권한 거부에서 실패를 주입합니다. 백업·안전 교체의 OS별 구현 가능성을 조사한 뒤 최소 수정합니다.
- **안전 불변식·수식:** `설치 실패 ⇒ 이전 검증 모델 사용 가능 ∨ 검증된 복구 사본 존재`; 성공 시 `SHA256(installed)==manifest_sha`.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests` + 신규 실패 주입 회귀
- **완료 기준:** 이전 모델이 소실되지 않고, 새 모델은 완전 검증 이후에만 로드 가능하며 실패 이유가 명확함
- **중단·원복 조건:** 새 교체 방식이 신뢰 가능한 해시·모델 버전·경로 계약을 변경하면 원복
- **관련 작업:** S-02, S-07
- **공식 기술 출처:** [기술 명세](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)

### S-13 · 로컬 오프라인 HTML의 읽기 용량·리소스 제한 (P1)

- **근거 소스:** [`src/cli/commands/novel/source.rs` L28-L55](https://github.com/P-JSL/Syousetu_Translate/blob/1f9bd9671b0a537f745bfccc768e91154f35003a/src/cli/commands/novel/source.rs#L28-L55)
- **현재 판정:** 네트워크 16MiB 제한과 로컬 `read_to_string`의 차이를 확인
- **확인된 코드 근거:** 네트워크 Fetcher는 응답을 최대 16MiB로 제한하지만, `--episode-html-dir`의 로컬 파일은 사전 용량 검사 없이 `fs::read_to_string()`으로 전부 읽습니다. 이후 8MiB window 한도는 이미 발생한 메모리 할당을 막지 못합니다.
- **구현·검증 방안:** 정상 장편 HTML/10~100MiB 합성 HTML/UTF-8 오류/권한 거부를 비교하고 peak RSS·오류 안내를 확인합니다. 상한이 필요하면 기존 정상 입력의 최대 길이를 먼저 조사합니다.
- **안전 불변식·수식:** `읽기 전 제한 != 처리 창 제한`; 전체 읽기 `RSS_peak ≥ 원문 할당 바이트`(추가 구조체·파서 오버헤드 제외).
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests` + 신규 실패 주입 회귀
- **완료 기준:** 로컬 대용량/손상 파일이 UI와 작업 큐 전체를 불안정하게 만들지 않으며 정상 입력의 번역 결과 불변
- **중단·원복 조건:** 기존 정상 오프라인 HTML을 임의로 거부하거나 회차 순서를 바꾸면 적용하지 않음
- **관련 작업:** S-05, S-06
- **공식 기술 출처:** [기술 명세](https://doc.rust-lang.org/std/fs/fn.read_to_string.html)

### S-14 · 출력 AtomicFile의 중간 종료·백업 복구 (P1)

- **근거 소스:** [`src/storage/atomic.rs` L50-L74](https://github.com/P-JSL/Syousetu_Translate/blob/1f9bd9671b0a537f745bfccc768e91154f35003a/src/storage/atomic.rs#L50-L74)
- **현재 판정:** 정상 오류 시 복구 구현은 존재, 강제 중단 시나리오는 미검증
- **확인된 코드 근거:** 현재 commit은 파일 flush/sync 후 기존 출력→`.backup` rename, 임시 출력→목적 경로 rename, 실패 시 백업 복구를 수행합니다. **프로세스 강제 종료 또는 전원 단절은 오류 반환 경로를 거치지 않습니다**.
- **구현·검증 방안:** 기존 `.backup` 존재/원본 백업 직후 강제 종료/새 파일 게시 직후 종료/이중 복구 실패를 임시 테스트 폴더에서 재현하고 고아 파일을 안전하게 식별합니다.
- **안전 불변식·수식:** `중단된 출력 ⇒ (기존 완전본 ∨ 새 완전본 ∨ 확인 가능한 완전 백업)`; 잘린 두 파일을 임의 합쳐 성공으로 처리 금지.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests` + 신규 실패 주입 회귀
- **완료 기준:** 이전 완전본 복구가 가능하거나 명시적인 수동 복구 경로가 남으며 완료된 회차 파일의 유실이 없음
- **중단·원복 조건:** 검증 없이 `.backup`나 임시 파일을 삭제하는 자동 정리 금지
- **관련 작업:** S-03, S-06
- **공식 기술 출처:** [기술 명세](https://learn.microsoft.com/en-us/windows/win32/fileio/moving-and-replacing-files)

### S-15 · 사용자 추가 CA 선택 경로와 TLS 오류 진단 (P2)

- **근거 소스:** [`src/http_client.rs` L1-L47](https://github.com/P-JSL/Syousetu_Translate/blob/1f9bd9671b0a537f745bfccc768e91154f35003a/src/http_client.rs#L1-L47)
- **현재 판정:** 설정/환경변수 CA 우선순위 확인; 악용·오설정 발생 미확인
- **확인된 코드 근거:** `custom_ca()`는 직접 지정값 외에 `SSL_CERT_FILE`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`에서 추가 CA 경로를 찾습니다. TLS 검증이 해제된 것은 아닙니다.
- **구현·검증 방안:** 정상 OS 루트/잘못된 PEM/환경변수 우선순위/기업 CA 체인/사용자 설정 CA를 테스트하고 선택한 CA 출처를 개인정보·인증정보 없이 진단하도록 검토합니다.
- **안전 불변식·수식:** `TLS 연결 허용 ⇒ 호스트명 일치 ∧ 인증서 체인 유효`; 잘못된 설정을 인증서 검증 우회로 해결하지 않습니다.
- **검증 명령:** `cargo test --locked --release --features gui --lib --bins --tests` + 신규 실패 주입 회귀
- **완료 기준:** OS 기본 신뢰 동작 유지, 비신뢰 인증서 거부, 추가 CA 선택 근거·오류가 정확히 표시됨
- **중단·원복 조건:** `danger_accept_invalid_certs`·무제한 도메인 신뢰 확대 금지
- **관련 작업:** S-05, S-07
- **공식 기술 출처:** [기술 명세](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html)

### 8.3 수행 순서 및 완료 판정

1. 최신 코드 및 기존 방어를 재확인하고, 합성 데이터/임시 폴더에서 실패를 먼저 재현합니다.
2. 사용자 설정·저장된 교정/모델·파일 경로를 유지한 채 필요한 최소 범위를 수정합니다.
3. Windows·Linux 환경별 실패/정상 경로를 실행·기록하고 원복을 시험합니다. 미실행 항목은 미검증으로 남깁니다.
4. 작업별 변경 파일·기존 동작·테스트 결과·위험·커밋 SHA를 보고합니다. 임의 배포·데이터 삭제 금지.

