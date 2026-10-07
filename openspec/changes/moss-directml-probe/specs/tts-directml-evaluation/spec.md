## ADDED Requirements

### Requirement: Explicit adapter evidence
The probe SHALL enumerate DXGI adapters with index, LUID, vendor and memory, and SHALL require an explicit compatible hardware adapter for DirectML.

#### Scenario: Multiple GPU vendors
- **WHEN** NVIDIA and AMD devices coexist
- **THEN** selecting the AMD index records that adapter and never silently selects NVIDIA or CPU

### Requirement: Real pipeline evidence
The probe SHALL reuse Nano generation and compare CPU and DirectML, saving valid EOS PCM, cold load, first PCM, RTF and cancellation/reuse results. Failures SHALL remain failures.

#### Scenario: DirectML executes only part of a graph
- **WHEN** ORT profiling reports CPU and DirectML nodes
- **THEN** the evidence reports both rather than claiming full GPU execution

### Requirement: Hardware-specific qualification
The probe SHALL distinguish adapter-specific results and SHALL NOT infer discrete GPU performance from an integrated GPU result.

#### Scenario: Local integrated GPU is slow
- **WHEN** local DirectML throughput is slower than CPU
- **THEN** the report retains that result and labels unavailable AMD discrete hardware as unverified
