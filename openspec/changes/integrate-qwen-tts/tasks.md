## Implementation

- [x] 1. Add pinned Candle adapter, verified model manifest, preset voices and source-preserving segmentation.
- [x] 2. Integrate backend-specific CPU/Metal device selection and worker configuration without coupling the reader to inference.
- [x] 3. Verify EOS/frame-limit handling, cancellation, protocol and backend feature combinations; simplify touched code and run quality checks.
- [x] 4. Run real-model CPU/Metal probes and UI checks; document measured results and unaccepted listening quality.
- [ ] 5. Human listening acceptance of Chinese, English, mixed text, soft newlines and long text including chunk boundaries.
