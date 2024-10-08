#[cfg(test)]
mod tests {
    use crate::helpers::*;
    use windows::Win32::Foundation::*;

    #[test]
    fn test_try_from_u32_for_sudo_mode() {
        assert_eq!(SudoMode::try_from(0), Ok(SudoMode::Disabled));
        assert_eq!(SudoMode::try_from(1), Ok(SudoMode::ForceNewWindow));
        assert_eq!(SudoMode::try_from(2), Ok(SudoMode::DisableInput));
        assert_eq!(SudoMode::try_from(3), Ok(SudoMode::Normal));
        assert_eq!(SudoMode::try_from(4), Err(ERROR_INVALID_PARAMETER.into()));
    }
    #[test]
    fn test_try_sudo_mode_to_u32() {
        assert_eq!(u32::from(SudoMode::Disabled), 0);
        assert_eq!(u32::from(SudoMode::ForceNewWindow), 1);
        assert_eq!(u32::from(SudoMode::DisableInput), 2);
        assert_eq!(u32::from(SudoMode::Normal), 3);
    }

    #[test]
    fn test_generate_rpc_endpoint_name() {
        assert_eq!(
            generate_rpc_endpoint_name(1234, 2345),
            r"sudo_elevate_1234_2345"
        );
    }
}
