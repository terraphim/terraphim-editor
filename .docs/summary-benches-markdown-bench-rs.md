# File Summary: benches/markdown_bench.rs

## File Path
`benches/markdown_bench.rs`

## Purpose
Performance benchmarking suite using Criterion to measure markdown-to-HTML conversion speed in native Rust environment (non-WASM). Provides baseline performance metrics for optimization efforts and regression testing.

## Key Components

### Benchmark Configuration
- **`criterion_group!(benches, benchmark_markdown)`** - Defines benchmark group
- **`criterion_main!(benches)`** - Entry point for Criterion benchmark runner
- Uses `#[bench]` harness = false in Cargo.toml to use Criterion instead of built-in benchmarking

### Test Data
- **`BENCHMARK_TEXT`** - Standardized markdown sample for consistent measurements
  - Same content as `tests/web.rs` for cross-environment comparison
  - Includes diverse markdown syntax:
    - 3 levels of headings
    - Bold and italic formatting
    - Nested lists (unordered and ordered)
    - Blockquotes
    - Fenced code blocks with language specification
    - Links
    - Tables
  - Represents realistic document complexity

## Benchmark Functions

### `benchmark_markdown(c: &mut Criterion)`
**Purpose**: Measures raw markdown conversion performance

**Implementation**:
- Creates `Options::default()` outside timing loop (warm-up)
- Uses `black_box()` to prevent compiler optimizations
- Calls `markdown::to_html_with_options()` repeatedly
- Criterion automatically:
  - Determines optimal iteration count
  - Calculates statistical metrics (mean, median, std dev)
  - Detects performance regressions
  - Generates detailed reports

**Metrics Captured**:
- Throughput (iterations per second)
- Mean execution time
- Standard deviation
- Confidence intervals
- Outlier detection

## Dependencies
- `criterion` (0.5) - Statistical benchmarking framework
  - Provides `black_box()` to prevent optimization
  - Handles warm-up, iteration counting, and statistical analysis
  - Generates HTML reports with graphs
- `markdown` - Same conversion library used in production code
- `terraphim_editor` - Project crate (for potential future benchmarks)

## Integration Points

### Execution
- **Command**: `cargo bench`
- **Output**: Console summary + HTML report in `target/criterion/`
- **Baseline Comparison**: Can save baselines for regression detection

### CI/CD Integration
- Can be integrated into automated performance testing
- Baseline comparisons catch performance regressions
- Results can be tracked over time

## Notable Patterns

### Black Box Pattern
```rust
markdown::to_html_with_options(black_box(BENCHMARK_TEXT), &options)
```
- Prevents compiler from optimizing away the actual work
- Ensures realistic measurements
- Forces full execution path

### Options Reuse
- Creates `Options::default()` once before benchmark
- Avoids measuring allocation/initialization overhead
- Focuses purely on conversion logic

## Performance Characteristics

### What This Measures
- **Pure Rust Performance**: Native code without WASM overhead
- **Conversion Algorithm Speed**: Core markdown parsing and HTML generation
- **Baseline for WASM Comparison**: Compare against `tests/web.rs` benchmark

### What This Doesn't Measure
- WASM runtime overhead
- DOM manipulation speed
- Event handler latency
- Browser-specific performance
- Memory allocation patterns

## Benchmark Results Interpretation

### Typical Metrics
- **Mean Time**: Average conversion time per iteration
- **Throughput**: Conversions per second
- **Variance**: Consistency of performance
- **Outliers**: Anomalous measurements (GC, system interruption)

### Comparison with Browser Tests
- Native benchmarks typically faster than WASM + DOM updates
- Browser test (`tests/web.rs`) measures end-to-end performance
- This benchmark isolates conversion logic

## Use Cases

### Development Workflow
1. **Optimization**: Measure impact of code changes
2. **Regression Detection**: Catch performance degradation
3. **Profiling Guide**: Identify hot paths for optimization
4. **Algorithm Comparison**: Evaluate alternative implementations

### Performance Goals
- Establish baseline performance metrics
- Set acceptable performance thresholds
- Track performance trends over project lifetime
- Validate optimization efforts

## Technical Considerations

### Benchmark Accuracy
- Criterion runs multiple iterations for statistical significance
- Outlier filtering removes anomalous measurements
- Warm-up iterations prevent cold-start bias
- Uses high-precision timers

### Platform Variations
- Performance varies by CPU, OS, and system load
- Important to benchmark on target deployment platforms
- Relative measurements more reliable than absolute

### Optimization Considerations
- Results guide optimization priorities
- Helps identify whether WASM overhead or algorithm is bottleneck
- Can benchmark individual functions by extracting them

## Improvement Opportunities
- Add benchmarks for different document sizes (small, medium, large, huge)
- Benchmark individual markdown features separately
- Add memory usage profiling
- Compare different markdown crate options
- Benchmark with different `Options` configurations
- Add worst-case scenarios (deeply nested structures, very long lines)
- Benchmark incremental updates vs full re-rendering
- Profile allocation patterns
