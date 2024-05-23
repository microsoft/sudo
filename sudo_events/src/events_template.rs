
use win_etw_macros::trace_logging_provider;
// Note: Generate GUID using TlgGuid.exe tool
#[trace_logging_provider(
    name = "Microsoft.Windows.Sudo",
    guid = "6ffdd42d-46d9-5efe-68a1-3b18cb73a607",
    provider_group_guid = "ffffffff-ffff-ffff-ffff-ffffffffffff"
)]
// tl:{6ffdd42d-46d9-5efe-68a1-3b18cb73a607}

pub trait SudoEvents {
    fn command_not_found(exe_name: &str);

    fn cmd_builtin_found(exe_name: &str);

    fn message(message: &str);

    fn run(
        exe_name: &str,
        requested_mode: u32,
        parent_pid: u32,
        redirected_input: bool,
        redirected_output: bool,
    );

    // TRACELOGGING EVENTS:
    //
    // These events need to add a PartA_PrivTags: u64 parameter to the end of
    // the event. That should be filled with PDT_ProductAndServicePerformance or
    // PDT_ProductAndServiceUsage. Our wrappers in tracing.rs should abstract
    // that away.
    //
    // Additionally, we manually set the keyword to MICROSOFT_KEYWORD_MEASURES.
    // However, we can't use that constant here, because the macro needs an
    // actual _literal_. So, we use the value 0x0000400000000000 directly.
    // MICROSOFT_KEYWORD_TELEMETRY is 0x0000200000000000, but that... doesn't work?

    // requested_mode:
    //   * 0: Use the allowed mode from the registry / policy
    //   * 1: Manually request forceNewWindow
    //   * 2: Manually request disableInput
    //   * 3: ??? We shouldn't get these. Requested mode is set by the CLI flags
    // allowed_mode: Straightforward. The mode in the registry
    // policy_mode: The mode set by the policy. If the policy isn't set, this should be 0xffffffff
    #[event(keyword = 0x0000400000000000)]
    fn modes(requested_mode: u32, allowed_mode: u32, policy_mode: u32, PartA_PrivTags: u64);
}
