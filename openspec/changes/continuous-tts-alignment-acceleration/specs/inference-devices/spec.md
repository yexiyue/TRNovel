# Spec Delta

## Purpose

提供独立的合成和对齐设备选择，通过实际运行校准、固定资源校验和可观察回退，保持原生 Rust 推理以及阅读器与模型运行时的解耦。

## ADDED Requirements

### Requirement: Native model compatibility

The system SHALL validate the pinned ONNX runtime against alignment model loading, inference and accuracy before integrating the model.

#### Scenario: Compatibility gate fails
- **WHEN** a required graph cannot execute correctly
- **THEN** model integration stops and the evidence is reported

### Requirement: Measured device selection

The system SHALL support CPU, optional CoreML and CUDA, and choose auto acceleration only after measured whole-pipeline benefit.

#### Scenario: No acceleration benefit
- **WHEN** an accelerator is slower or unavailable
- **THEN** auto selects CPU and reports the decision

### Requirement: Immutable accelerator resources

The system SHALL pin all model resources and verify size and SHA256, report actual providers and maintain device-specific caches.

#### Scenario: Explicit unavailable device
- **WHEN** a user selects an unavailable accelerator
- **THEN** preparation reports an actionable error without silently substituting a device
