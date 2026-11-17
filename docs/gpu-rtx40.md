# NVIDIA GPU 빌드 사용 안내

RTX 4070 이상급 NVIDIA GPU를 대상으로 한 별도 CUDA 빌드입니다. **실장비
번역 품질·속도·VRAM 검증은 아직 수행되지 않았으며, 85~95% 단축을 보장하지
않습니다.** 일반 CPU 패키지와 기본 CI·릴리즈에는 CUDA 라이브러리가 추가되지
않습니다.

## 계산 방식

디스크에는 기존 체크섬이 고정된 SMaLL-100 INT8 모델을 그대로 사용합니다.
GPU 경로는 로딩할 때 한 번 FP16 가중치로 풀어 GPU에 유지하고, Tensor Core를
이용한 행렬 계산을 사용합니다. CPU와 정밀도·누적 방식이 달라 번역 결과가
바이트 단위로 같다고 가정하지 않습니다. 정규화와 softmax는 F32로 처리하며,
토큰 선택은 전체 어휘를 대상으로 합니다. 모델 파일을 새로 학습하거나
원본 FP32 모델 전체를 내려받을 필요는 없습니다.

GPU에서는 초기 CPU 단일·배치 비교를 생략하고 배치 경로를 사용합니다.
`--parallel-mode serial`을 명시하면 단일 경로를 사용할 수 있습니다.
CPU/GPU 및 드라이버 간 결과가 섞이지 않도록 GPU의 디스크 결과 재사용은
비활성화하며, 현재 작업 안의 중복 문장 합치기는 유지합니다.

## 수동 빌드 아티팩트

GitHub Actions에서 **Build optional NVIDIA CUDA overlay** 워크플로를 수동으로
실행합니다. 이 작업은 CUDA 12.8.1과 compute capability 8.9용 PTX를 사용해
Windows x64 실행 파일을 컴파일합니다. RTX 4070을 포함한 Ada GPU가 기준이며,
이후 GPU에서는 호환 드라이버의 PTX JIT 지원과 실제 실행을 확인해야 합니다.
호스티드 Windows 러너에는 GPU가 없어 이 작업의 성공은 **컴파일·패키징 확인**입니다.

완료 후 `SyousetuTranslate-CUDA-Windows-x64-<커밋 SHA>` 아티팩트를 받아 새
폴더에 풉니다. 기존 CPU 설치 폴더에 덮어쓰지 마십시오. 이 아티팩트는 모델을
중복 전송하지 않는 **GPU 실행 파일 묶음**입니다. 기존 포터블 릴리즈의
`models/ja-ko` 폴더를 새 폴더의 `models/ja-ko`로 복사하면 동일 모델을 바로
사용할 수 있습니다. 설정과 사용자 교정 기억도 필요한 경우 기존 파일을
보존한 채 별도로 복사합니다. 모델이 없으면 앱의 기존 모델 준비 기능을
사용할 수 있습니다.

묶음에는 실행 파일이 실제로 가져오는 CUDA 라이브러리와 필요한 전이 의존성만
포함됩니다. NVRTC가 필요한 경우 동적으로 읽는 builtins 라이브러리도 포함합니다.
전체 CUDA Toolkit, 컴파일러, Python, 사용하지 않는 NVIDIA 라이브러리나
NVIDIA 디스플레이 드라이버는 포함하지 않습니다. 라이브러리 목록은
`CUDA_RUNTIME_DLLS.txt`, NVIDIA 약관은 `licenses/NVIDIA-CUDA-EULA.txt`에 있습니다.

## 실행

최신 NVIDIA Windows 드라이버를 설치하십시오. CUDA 12.8.1 Toolkit과 함께
제공되는 Windows 드라이버 기준은 572.61이며, GPU 세대에 따라 더 최신 드라이버가
필요할 수 있습니다. 설치된 드라이버가 해당 Toolkit/PTX를 지원하는지 확인합니다.
일반 사용자는 CUDA Toolkit 전체나 Rust 컴파일러를 설치할 필요가 없습니다.
Windows용 Microsoft Visual C++ x64 재배포 런타임도 필요합니다.

GUI 설정에서 장치를 `cuda`로 선택하고 GPU 번호를 기본값 0으로 두면 CPU로
전환되지 않고 GPU 초기화 오류를 확인할 수 있습니다. CLI 예시는 다음과 같습니다.

```powershell
.\syousetu-translate.exe file --input .\original.txt --out .\translated-gpu.txt --device cuda --cuda-device 0 --jobs 128 --parallel-mode parallel --no-learning --no-result-cache
```

`auto`는 CUDA를 사용할 수 있으면 GPU를 선택하고 초기화할 수 없으면 CPU로
처리하도록 요청하는 모드입니다. 다만 이 CUDA 실행 파일은 NVIDIA DLL을
정적으로 가져올 수 있어, **드라이버 DLL이 전혀 없는 PC에서는 Windows가
프로그램 시작 자체를 막을 수 있습니다**. 이런 PC에서는 일반 CPU 패키지를
사용하십시오. 번역이 시작된 뒤 OOM·CUDA 오류가 나면 완료된 TXT prefix를
보존하고 실패를 표시하며, 전체 원고를 조용히 다시 번역하지 않습니다.

RTX 4070의 12 GB VRAM을 기준으로 먼저 64~128개 논리 문맥을 사용하십시오.
`--jobs`는 GPU 장치 수나 CUDA 스트림 수가 아니라 배치 문맥 수입니다. 다른
프로그램이 VRAM을 사용하거나 긴 문장이 많으면 문맥 수를 낮추십시오.

## 소스에서 직접 빌드

Rust stable, Visual Studio C++ 도구, CUDA Toolkit 12.8.1을 준비합니다. Toolkit에
맞는 MSVC 호스트 컴파일러가 필요합니다. 수동 워크플로는 호환되는 14.2x/14.3x
도구를 선택하며, 없으면 Visual Studio의 14.38 도구를 추가합니다.

```powershell
$env:CUDA_COMPUTE_CAP = '89'
# 필요한 경우 실제 설치된 CUDA 호환 cl.exe 경로를 지정합니다.
# $env:NVCC_CCBIN = 'C:\...\MSVC\14.38.x\bin\Hostx64\x64\cl.exe'
cargo build --locked --release --features gui,cuda --bins --target x86_64-pc-windows-msvc
rustc --edition=2021 tools/cuda_package.rs -o cuda-package.exe
.\cuda-package.exe target/x86_64-pc-windows-msvc/release "$env:CUDA_PATH" dist/cuda-overlay
```

## 실장비 확인 범위

비교할 때 CPU와 GPU에서 같은 모델·원고·화 범위·문맥 수를 사용합니다.
`--no-learning --no-result-cache`로 재번역 결과를 배제하고 새 출력 파일을
사용하십시오. 모델 준비·다운로드 시간과 추론 시간을 구분하고, 전체 TXT의
화 수·순서·누락을 확인합니다. 속도 반복 측정은 조건별 최대 2회로 제한합니다.
인명·수치·부정문·대화체·줄임말 등 실제 번역 품질과 CUDA 오류/OOM도 확인해야
합니다. CPU와 GPU의 출력이 다르다는 사실만으로 어느 쪽이 더 정확하다고
판정하지 않습니다.
