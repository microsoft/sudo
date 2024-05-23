# This is a list of what I think all the languages we need to support are. At
# the very least, it's all the languages that the Terminal's context menu are
# localized into. If we need more than that, go ahead and add more. This has to
# be the most complete list - the script that actually generates the .rc file
# will use whatever subest of languages are actually available.

$languageCodes = @(
"af-ZA",
"am-ET",
"ar-SA",
"as-IN",
"az-Latn-AZ",
"bg-BG",
"bn-IN",
"bs-Latn-BA",
"ca-ES",
"ca-Es-VALENCIA",
"cs-CZ",
"cy-GB",
"da-DK",
"de-DE",
"el-GR",
"en-GB",
"en-US",
"es-ES",
"es-MX",
"et-EE",
"eu-ES",
"fa-IR",
"fi-FI",
"fil-PH",
"fr-CA",
"fr-FR",
"ga-IE",
"gd-gb",
"gl-ES",
"gu-IN",
"he-IL",
"hi-IN",
"hr-HR",
"hu-HU",
"hy-AM",
"id-ID",
"is-IS",
"it-IT",
"ja-JP",
"ka-GE",
"kk-KZ",
"km-KH",
"kn-IN",
"ko-KR",
"kok-IN",
"lb-LU",
"lo-LA",
"lt-LT",
"lv-LV",
"mi-NZ",
"mk-MK",
"ml-IN",
"mr-IN",
"ms-MY",
"mt-MT",
"nb-NO",
"ne-NP",
"nl-NL",
"nn-NO",
"or-IN",
"pa-IN",
"pl-PL",
"pt-BR",
"pt-PT",
"qps-ploc",
"qps-ploca",
"qps-plocm",
"quz-PE",
"ro-RO",
"ru-RU",
"sk-SK",
"sl-SI",
"sq-AL",
"sr-Cyrl-BA",
"sr-Cyrl-RS",
"sr-Latn-RS",
"sv-SE",
"ta-IN",
"te-IN",
"th-TH",
"tr-TR",
"tt-RU",
"ug-CN",
"uk-UA",
"ur-PK",
"uz-Latn-UZ",
"vi-VN",
"zh-CN",
"zh-TW"
)

function Get-Language-Sublanguage-Constants {
    param (
        [int]$lcid
    )

    $language = $lcid -band 0xFFFF
    $sublanguage = ($lcid -shr 16) -band 0x3F

    # Language Constants
    $languageConstant = "LANG_" + ([System.Globalization.CultureInfo]::GetCultureInfo($lcid).TwoLetterISOLanguageName).ToUpper()

    # Sublanguage Constants
    $sublanguageConstant = "SUBLANG_" + ([System.Globalization.CultureInfo]::GetCultureInfo($lcid).Name.Split('-')[1]).ToUpper()

    return  $language, $sublanguage, $languageConstant, $sublanguageConstant, ([System.Globalization.CultureInfo]::GetCultureInfo($lcid).ThreeLetterISOLanguageName).ToUpper()
}

$languageHashTable = @{}

foreach ($code in $languageCodes) {
    $cultureInfo = New-Object System.Globalization.CultureInfo $code
    $displayName = $cultureInfo.DisplayName
    $neutralCulture = $cultureInfo.Parent.Name
    $lcid = $cultureInfo.LCID
    $language, $sublanguage, $languageConstant, $sublanguageConstant, $threeLetter = Get-Language-Sublanguage-Constants -lcid $lcid

    # trim out "LANG_" and "SUBLANG_
    $languageConstant = $languageConstant.Substring(5)
    $sublanguageConstant = $sublanguageConstant.Substring(8)

    $hashTableValue = @(
        # $cultureInfo.Name.ToUpper(),
        # "",
        $threeLetter,
        $language,
        $sublanguage, # $cultureInfo.Name.ToUpper() + '_' + $cultureInfo.Parent.Name.ToUpper(),
        $displayName
    )

    $languageHashTable[$code] = $hashTableValue
}
# write list like:
#
#  "eu-ES" = @("EUQ", "BASQUE", "DEFAULT", "Basque (Basque)");
# $languageHashTable | % {

foreach ($code in $languageCodes) {
    $lang = $languageHashTable[$code]
    # Get language and sublanguage constants

    write-host " `"$($code)`" = @(`"$($lang[0])`", `"$($lang[1])`", `"$($lang[2])`", `"$($lang[3])`");"

    # if ($_.Value -ne $null ) {
    #     write-host " `"$($_.Key)`" = @(`"$($_.Value[0])`", `"$($_.Value[1])`", `"$($_.Value[2])`", `"$($_.Value[3])`");" }
}
