#![no_main]

use ca_mcp::{
    AnalyzeImpactRequest, BuildContextRequest, CancelJobRequest, FindReferencesRequest,
    ForgetMemoryRequest, GetFileOutlineRequest, GetRepoMapRequest, GetSymbolRequest,
    IndexRepositoryRequest, JobStatusRequest, ReadCodeRequest, RepositoryStatusRequest,
    SearchMemoriesRequest, SearchSymbolsRequest, TraceCallsRequest, UpsertMemoryRequest,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let bounded = &data[..data.len().min(128 * 1_024)];
    macro_rules! decode_all {
        ($($kind:ty),+ $(,)?) => {
            $(let _ = serde_json::from_slice::<$kind>(bounded);)+
        };
    }
    decode_all!(
        RepositoryStatusRequest,
        IndexRepositoryRequest,
        JobStatusRequest,
        CancelJobRequest,
        SearchSymbolsRequest,
        GetSymbolRequest,
        FindReferencesRequest,
        TraceCallsRequest,
        GetFileOutlineRequest,
        ReadCodeRequest,
        GetRepoMapRequest,
        AnalyzeImpactRequest,
        BuildContextRequest,
        SearchMemoriesRequest,
        UpsertMemoryRequest,
        ForgetMemoryRequest,
    );
});
