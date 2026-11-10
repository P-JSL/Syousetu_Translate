# This applies to oneDNN; CTranslate2 also builds its required non-graph ops.
set(ONEDNN_ENABLE_PRIMITIVE "MATMUL;REORDER" CACHE STRING "Benchmark primitives")
set(ONEDNN_BUILD_GRAPH OFF CACHE BOOL "No graph compiler required")
