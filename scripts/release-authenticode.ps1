# The 0.4.0 release exception requires an explicit opt-in and exact approved bytes.
# Later versions and all non-opt-in builds still require timestamped Authenticode.
function Assert-ReleaseAuthenticode {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][ValidateSet('Installer','App','Worker')][string]$Role,
        [switch]$AllowUnsigned040
    )
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($AllowUnsigned040) {
        $approved = @{
            Installer = '0F2E944EFAEA6C2106D4FF91BDAA05BE63019CF6C34E63BF03AF9E46FEE53576'
            App = '341349C4C2A5A8DC5CF3F4D39A0004236397629145C6E7F9C240594BB75E18F5'
            Worker = 'F49064F886AE99FDFE3D8A76D733583BE153BCCD3FFA68D1941A65419A0DFE5F'
        }
        if ($Version -cne '0.4.0' -or $signature.Status -ne 'NotSigned' -or
            (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $approved[$Role]) {
            throw "The unsigned exception only covers the approved 0.4.0 $Role bytes."
        }
        return
    }
    if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate) {
        throw "Expected a valid timestamped Authenticode signature on $Path. Got $($signature.Status)."
    }
}
