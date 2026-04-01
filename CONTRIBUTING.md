# Contributing

## Scope of Contributions

Contributions should reinforce the repository's current direction:

- deterministic fixed-timestep execution
- explicit state progression
- stable update ordering
- reproducible simulation behavior
- measurable performance work

Changes that expand the repository into unrelated areas should be proposed before implementation.

## Development Setup

Configure and build:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build --parallel
```

Run tests:

```bash
ctest --test-dir build --output-on-failure
```

Run benchmarks:

```bash
./build/benchmarks/xenor_engine_benchmarks
```

Run the example:

```bash
./build/examples/xenor_engine_resource_pipeline_example
```

## Engineering Expectations

- Keep deterministic behavior obvious from the code.
- Prefer explicit value flow over hidden side effects.
- Preserve stable execution ordering unless a change is intentional and documented.
- Avoid adding runtime behavior that depends on wall-clock time.
- Keep abstractions small and justified.
- Do not add speculative framework layers.

## Code Style

- Use modern C++20 with restrained language features.
- Prefer descriptive names over compressed or clever forms.
- Add comments only when they explain intent that is not already visible in the code.
- Keep public headers focused on stable engine-facing types.

## Verification

Before proposing a change, run:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
ctest --test-dir build --output-on-failure
```

If benchmark-relevant code changes, also build and run the benchmark target.

## Commit Messages

Use concise, descriptive commit messages that state the technical change clearly.

Examples:

- `Add overflow checks to simulation clock`
- `Extend repeatability coverage for ordered systems`
- `Refine benchmark workload setup`
