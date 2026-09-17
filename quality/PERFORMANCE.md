# Performance validation

RepoSlice measures cold and warm scan performance with the CLI `benchmark` command.

```powershell
cargo run -p reposlice-cli -- benchmark D:\projects\spring-petclinic --runs 5
```

JSON output:

```powershell
cargo run -q -p reposlice-cli -- benchmark D:\projects\spring-petclinic --runs 5 --format json
```

The benchmark reports cold scan duration, warm average duration, files per second, and every individual run. Warm scans of clean Git repositories can reuse the commit-aware in-memory model cache. Dirty repositories always fall back to the full fingerprint path so uncommitted changes are not hidden.

The scheduled `real-corpus-and-performance` workflow records Linux and Windows baselines against Spring PetClinic. It rejects invalid throughput and warm scans that regress beyond a 25% noise budget relative to the cold scan. Performance changes should also be compared against the previous artifacts before merging analyzer changes that affect repository traversal or source caching.
