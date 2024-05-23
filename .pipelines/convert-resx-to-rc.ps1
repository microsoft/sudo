# This script is used to move the resources from all the resx files in the
# directory (args[0]) to a .rc and .h file for use in C++ projects. (and a rust
# file for loading resources in Rust)

# Root directory which contains the resx files
$parentDirectory = $args[0]

# File name of the base resource.h which contains all the non-localized resource definitions
$baseHeaderFileName = $args[1]

# Target file name of the resource header file, which will be used in code - Example: resource.h
$generatedHeaderFileName = $args[2]

# File name of the base ProjectName.rc which contains all the non-localized resources
$baseRCFileName = $args[3]

# Target file name of the resource rc file, which will be used in code - Example: ProjectName.rc
$generatedRCFileName = $args[4]

# Target file name of the rust resource file, which will be used in code - Example: resource_ids.rs
$generatedRustFileName = $args[5]

# Optional argument: Initial resource id in the resource header file. By default it is 101
if ($args.Count -eq 7)
{
    $initResourceID = $args[6]
}
else
{
    $initResourceID = 101
}

# Flags to check if the first updated has occurred
$rcFileUpdated = $false
# $rustFileUpdated = $false

# Output folder for the new resource files. It will be in ProjectDir\Generated Files so that the files are ignored by .gitignore
$generatedFilesFolder = $parentDirectory + "\Generated Files"

# Create Generated Files folder if it doesn't exist
if (!(Test-Path -Path $generatedFilesFolder))
{
    $paramNewItem = @{
        Path      = $generatedFilesFolder
        ItemType  = 'Directory'
        Force     = $true
    }

    New-Item @paramNewItem
}

# Hash table to get the language codes from the code used in the file name
$languageHashTable = @{
                        # This is the table straight from PowerToys

                        # "ar" = @("ARA", "ARABIC", "NEUTRAL", "Arabic");
                        # "bg" = @("BGR", "BULGARIAN", "NEUTRAL", "Bulgarian");
                        # "ca" = @("CAT", "CATALAN", "NEUTRAL", "Catalan");
                        # "cs" = @("CSY", "CZECH", "NEUTRAL", "Czech");
                        # "de" = @("DEU", "GERMAN", "NEUTRAL", "German");
                        # "en-US" = @("ENU", "ENGLISH", "ENGLISH_US", "English (United States)");
                        # "es" = @("ESN", "SPANISH", "NEUTRAL", "Spanish");
                        # "eu-ES" = @("EUQ", "BASQUE", "DEFAULT", "Basque (Basque)");
                        # "fr" = @("FRA", "FRENCH", "NEUTRAL", "French");
                        # "he" = @("HEB", "HEBREW", "NEUTRAL", "Hebrew");
                        # "hu" = @("HUN", "HUNGARIAN", "NEUTRAL", "Hungarian");
                        # "it" = @("ITA", "ITALIAN", "NEUTRAL", "Italian");
                        # "ja" = @("JPN", "JAPANESE", "NEUTRAL", "Japanese");
                        # "ko" = @("KOR", "KOREAN", "NEUTRAL", "Korean");
                        # "nb-NO" = @("NOR", "NORWEGIAN", "NORWEGIAN_BOKMAL", "Norwegian Bokmål (Norway)");
                        # "nl" = @("NLD", "DUTCH", "NEUTRAL", "Dutch");
                        # "pl" = @("PLK", "POLISH", "NEUTRAL", "Polish");
                        # "pt-BR" = @("PTB", "PORTUGUESE", "PORTUGUESE_BRAZILIAN", "Portuguese (Brazil)");
                        # "pt-PT" = @("PTG", "PORTUGUESE", "PORTUGUESE", "Portuguese (Portugal)");
                        # "ro" = @("ROM", "ROMANIAN", "NEUTRAL", "Romanian");
                        # "ru" = @("RUS", "RUSSIAN", "NEUTRAL", "Russian");
                        # "sk" = @("SKY", "SLOVAK", "NEUTRAL", "Slovak");
                        # "sv" = @("SVE", "SWEDISH", "NEUTRAL", "Swedish");
                        # "tr" = @("TRK", "TURKISH", "NEUTRAL", "Turkish");
                        # "zh-CN" =  @("CHS", "CHINESE", "NEUTRAL", "Chinese (Simplified)");
                        # "zh-Hans" =  @("CHS", "CHINESE", "NEUTRAL", "Chinese (Simplified)");
                        # "zh-Hant" = @("CHT", "CHINESE", "CHINESE_TRADITIONAL", "Chinese (Traditional)")
                        # "zh-TW" = @("CHT", "CHINESE", "CHINESE_TRADITIONAL", "Chinese (Traditional)")

# GENERATE ME WITH gen-lang-codes.ps1
#
# the numbers in params 1 and 2 are the language and sublanguage values for the
# rc file's LANGUAGE statement. Usually those are defined in windows.h, but we
# can't just figure out what the constant in that file is just from a language
# code. I suppose this script could probably me modified to generate these
# values on the fly from the same code in gen-lang-codes.ps1, but I'm not sure
# it's worth it.
 "af-ZA" = @("AFR", "1078", "0", "Afrikaans (South Africa)");
 "am-ET" = @("AMH", "1118", "0", "Amharic (Ethiopia)");
 "ar-SA" = @("ARA", "1025", "0", "Arabic (Saudi Arabia)");
 "as-IN" = @("ASM", "1101", "0", "Assamese (India)");
 "az-Latn-AZ" = @("AZE", "1068", "0", "Azerbaijani (Latin, Azerbaijan)");
 "bg-BG" = @("BUL", "1026", "0", "Bulgarian (Bulgaria)");
 "bn-IN" = @("BEN", "1093", "0", "Bangla (India)");
 "bs-Latn-BA" = @("BOS", "5146", "0", "Bosnian (Latin, Bosnia & Herzegovina)");
 "ca-ES" = @("CAT", "1027", "0", "Catalan (Spain)");
 "ca-Es-VALENCIA" = @("CAT", "2051", "0", "Catalan (Spain, Valencian)");
 "cs-CZ" = @("CES", "1029", "0", "Czech (Czechia)");
 "cy-GB" = @("CYM", "1106", "0", "Welsh (United Kingdom)");
 "da-DK" = @("DAN", "1030", "0", "Danish (Denmark)");
 "de-DE" = @("DEU", "1031", "0", "German (Germany)");
 "el-GR" = @("ELL", "1032", "0", "Greek (Greece)");
 "en-GB" = @("ENG", "2057", "0", "English (United Kingdom)");
 "en-US" = @("ENG", "1033", "0", "English (United States)");
 "es-ES" = @("SPA", "3082", "0", "Spanish (Spain)");
 "es-MX" = @("SPA", "2058", "0", "Spanish (Mexico)");
 "et-EE" = @("EST", "1061", "0", "Estonian (Estonia)");
 "eu-ES" = @("EUS", "1069", "0", "Basque (Spain)");
 "fa-IR" = @("FAS", "1065", "0", "Persian (Iran)");
 "fi-FI" = @("FIN", "1035", "0", "Finnish (Finland)");
 "fil-PH" = @("FIL", "1124", "0", "Filipino (Philippines)");
 "fr-CA" = @("FRA", "3084", "0", "French (Canada)");
 "fr-FR" = @("FRA", "1036", "0", "French (France)");
 "ga-IE" = @("GLE", "2108", "0", "Irish (Ireland)");
 "gd-gb" = @("GLA", "1169", "0", "Scottish Gaelic (United Kingdom)");
 "gl-ES" = @("GLG", "1110", "0", "Galician (Spain)");
 "gu-IN" = @("GUJ", "1095", "0", "Gujarati (India)");
 "he-IL" = @("HEB", "1037", "0", "Hebrew (Israel)");
 "hi-IN" = @("HIN", "1081", "0", "Hindi (India)");
 "hr-HR" = @("HRV", "1050", "0", "Croatian (Croatia)");
 "hu-HU" = @("HUN", "1038", "0", "Hungarian (Hungary)");
 "hy-AM" = @("HYE", "1067", "0", "Armenian (Armenia)");
 "id-ID" = @("IND", "1057", "0", "Indonesian (Indonesia)");
 "is-IS" = @("ISL", "1039", "0", "Icelandic (Iceland)");
 "it-IT" = @("ITA", "1040", "0", "Italian (Italy)");
 "ja-JP" = @("JPN", "1041", "0", "Japanese (Japan)");
 "ka-GE" = @("KAT", "1079", "0", "Georgian (Georgia)");
 "kk-KZ" = @("KAZ", "1087", "0", "Kazakh (Kazakhstan)");
 "km-KH" = @("KHM", "1107", "0", "Khmer (Cambodia)");
 "kn-IN" = @("KAN", "1099", "0", "Kannada (India)");
 "ko-KR" = @("KOR", "1042", "0", "Korean (Korea)");
 "kok-IN" = @("KOK", "1111", "0", "Konkani (India)");
 "lb-LU" = @("LTZ", "1134", "0", "Luxembourgish (Luxembourg)");
 "lo-LA" = @("LAO", "1108", "0", "Lao (Laos)");
 "lt-LT" = @("LIT", "1063", "0", "Lithuanian (Lithuania)");
 "lv-LV" = @("LAV", "1062", "0", "Latvian (Latvia)");
 "mi-NZ" = @("MRI", "1153", "0", "Māori (New Zealand)");
 "mk-MK" = @("MKD", "1071", "0", "Macedonian (North Macedonia)");
 "ml-IN" = @("MAL", "1100", "0", "Malayalam (India)");
 "mr-IN" = @("MAR", "1102", "0", "Marathi (India)");
 "ms-MY" = @("MSA", "1086", "0", "Malay (Malaysia)");
 "mt-MT" = @("MLT", "1082", "0", "Maltese (Malta)");
 "nb-NO" = @("NOB", "1044", "0", "Norwegian Bokmål (Norway)");
 "ne-NP" = @("NEP", "1121", "0", "Nepali (Nepal)");
 "nl-NL" = @("NLD", "1043", "0", "Dutch (Netherlands)");
 "nn-NO" = @("NNO", "2068", "0", "Norwegian Nynorsk (Norway)");
 "or-IN" = @("ORI", "1096", "0", "Odia (India)");
 "pa-IN" = @("PAN", "1094", "0", "Punjabi (India)");
 "pl-PL" = @("POL", "1045", "0", "Polish (Poland)");
 "pt-BR" = @("POR", "1046", "0", "Portuguese (Brazil)");
 "pt-PT" = @("POR", "2070", "0", "Portuguese (Portugal)");
 "qps-ploc" = @("", "1281", "0", "qps (Ploc)");
 "qps-ploca" = @("", "1534", "0", "qps (PLOCA)");
 "qps-plocm" = @("", "2559", "0", "qps (PLOCM)");
 "quz-PE" = @("", "3179", "0", "Quechua (Peru)");
 "ro-RO" = @("RON", "1048", "0", "Romanian (Romania)");
 "ru-RU" = @("RUS", "1049", "0", "Russian (Russia)");
 "sk-SK" = @("SLK", "1051", "0", "Slovak (Slovakia)");
 "sl-SI" = @("SLV", "1060", "0", "Slovenian (Slovenia)");
 "sq-AL" = @("SQI", "1052", "0", "Albanian (Albania)");
 "sr-Cyrl-BA" = @("SRP", "7194", "0", "Serbian (Cyrillic, Bosnia & Herzegovina)");
 "sr-Cyrl-RS" = @("SRP", "10266", "0", "Serbian (Cyrillic, Serbia)");
 "sr-Latn-RS" = @("SRP", "9242", "0", "Serbian (Latin, Serbia)");
 "sv-SE" = @("SWE", "1053", "0", "Swedish (Sweden)");
 "ta-IN" = @("TAM", "1097", "0", "Tamil (India)");
 "te-IN" = @("TEL", "1098", "0", "Telugu (India)");
 "th-TH" = @("THA", "1054", "0", "Thai (Thailand)");
 "tr-TR" = @("TUR", "1055", "0", "Turkish (Türkiye)");
 "tt-RU" = @("TAT", "1092", "0", "Tatar (Russia)");
 "ug-CN" = @("UIG", "1152", "0", "Uyghur (China)");
 "uk-UA" = @("UKR", "1058", "0", "Ukrainian (Ukraine)");
 "ur-PK" = @("URD", "1056", "0", "Urdu (Pakistan)");
 "uz-Latn-UZ" = @("UZB", "1091", "0", "Uzbek (Latin, Uzbekistan)");
 "vi-VN" = @("VIE", "1066", "0", "Vietnamese (Vietnam)");
 "zh-CN" = @("ZHO", "2052", "0", "Chinese (China)");
 "zh-TW" = @("ZHO", "1028", "0", "Chinese (Taiwan)");

                        }

# Store the content to be written to a buffer
$rcFileContent = ""

# Start by pre-populating the header file with a warning and the contents of the
# base header file. Do this only once, we'll append generated content to this
# later.
$headerFileContent = "// This file was auto-generated. Changes to this file may cause incorrect behavior and will be lost if the code is regenerated.`r`n"
$rustFileContent = $headerFileContent; # The rust file doesn't have a base currently. We didn't need one.
try {
    $headerFileContent += (Get-Content $parentDirectory\$baseHeaderFileName  -Raw)
}
catch {
    echo "Failed to read base header file."
    exit 0
}

$lastResourceID = $initResourceID
# Iterate over all resx files in parent directory
Get-ChildItem $parentDirectory -Recurse -Filter *.resw |
Foreach-Object {
    Write-Host "Processing $($_.FullName)"
    $xmlDocument = $null
    try {
        $xmlDocument = [xml](Get-Content $_.FullName -ErrorAction:Stop)
    }
    catch {
        Write-Host "Failed to load $($_.FullName)"
        exit 0
    }

    # Get language code from file name
    $lang = "en"
    $tokens = $_.Name -split "\."
    if ($tokens.Count -eq 3) {
        $lang = $tokens[1]
    } else {
        $d = $_.Directory.Name
        If ($d.Contains('-')) { # Looks like a language directory
            $lang = $d
        }
    }

    $langData = $languageHashTable[$lang]
    if ($null -eq $langData -and $lang.Contains('-')) {
        # Modern Localization comes in with language + country tuples;
        # we want to detect the language alone if we don't support the language-country
        # version.
        $lang = ($lang -split "-")[0]
        $langData = $languageHashTable[$lang]
    }
    if ($null -eq $langData) {
        Write-Warning "Unknown language $lang"
        Return
    }

    $newLinesForRCFile = ""
    $newLinesForHeaderFile = ""
    $newLinesForRustFile = ""

    try {
        foreach ($entry in $xmlDocument.root.data) {
            $culture = [System.Globalization.CultureInfo]::GetCultureInfo('en-US')
            # Each resource is named as IDS_ResxResourceName, in uppercase. Escape occurrences of double quotes in the string
            $lineInRCFormat = "IDS_" + $entry.name.ToUpper($culture) + " L`"" + $entry.value.Replace("`"", "`"`"") + "`""
            $newLinesForRCFile = $newLinesForRCFile + "`r`n    " + $lineInRCFormat

            # Resource header & rust file needs to be updated only for one
            # language - en-US, where our strings are first authored. This is to
            # avoid duplicate entries in the header file.
            if ($lang -eq "en-US") {
                $lineInHeaderFormat = "#define IDS_" + $entry.name.ToUpper($culture) + " " + $lastResourceID.ToString()
                $newLinesForHeaderFile = $newLinesForHeaderFile + "`r`n" + $lineInHeaderFormat

                $lineInRustFormat = "string_resources! { IDS_$($entry.name.ToUpper($culture)) = $($lastResourceID.ToString()); }"

                $newLinesForRustFile = $newLinesForRustFile + "`r`n" + $lineInRustFormat

                $lastResourceID++
            }
        }
    }
    catch {
        echo "Failed to read XML document."
        exit 0
    }

    if ($newLinesForRCFile -ne "") {
        # Add string table syntax
        $newLinesForRCFile = "`r`nSTRINGTABLE`r`nBEGIN" + $newLinesForRCFile + "`r`nEND"

        $langStart = "`r`n/////////////////////////////////////////////////////////////////////////////`r`n// " + $langData[3]  + " resources`r`n`r`n"
        # $langStart += "#if !defined(AFX_RESOURCE_DLL) || defined(AFX_TARG_" + $langData[0] + ")`r`nLANGUAGE LANG_" + $langData[1] + ", SUBLANG_" + $langData[2] + "`r`n"
        $langStart += "#if !defined(AFX_RESOURCE_DLL) || defined(AFX_TARG_" + $langData[0] + ")`r`nLANGUAGE " + $langData[1] + ", " + $langData[2] + "`r`n"

        $langEnd = "`r`n`r`n#endif    // " + $langData[3] + " resources`r`n/////////////////////////////////////////////////////////////////////////////`r`n"

        $newLinesForRCFile = $langStart + $newLinesForRCFile + $langEnd
    }

    # Initialize the rc file with an auto-generation warning and content from the base rc
    if (!$rcFileUpdated) {
        $rcFileContent = "// This file was auto-generated. Changes to this file may cause incorrect behavior and will be lost if the code is regenerated.`r`n"
        try {
            $rcFileContent += (Get-Content $parentDirectory\$baseRCFileName -Raw)
        }
        catch {
            echo "Failed to read base rc file."
            exit 0
        }
        $rcFileUpdated = $true
    }

    # Add in the new string table to the rc file
    $rcFileContent += $newLinesForRCFile

    # Here we deviate more from the original script. We've got multiple resw
    # files to source, and we need to include the resource IDs from all of them
    # in the final header (and .rs file).
    #
    # Our main resw file is the en-US one, so we only ever assembled additional
    # header & rust lines in the case that the language for this resw file was
    # en-US.
    #
    # Now that we have those lines, stick them in the header and rust files.

    $headerFileContent += $newLinesForHeaderFile
    $rustFileContent += $newLinesForRustFile
}

# Write to header file if the content has changed or if the file doesnt exist
try {
    if (!(Test-Path -Path $generatedFilesFolder\$generatedHeaderFileName) -or (($headerFileContent + "`r`n") -ne (Get-Content $generatedFilesFolder\$generatedHeaderFileName -Raw))) {
        Set-Content -Path $generatedFilesFolder\$generatedHeaderFileName -Value $headerFileContent  -Encoding "utf8"
    }
    else {
        # echo "Skipping write to generated header file"
    }
}
catch {
    echo "Failed to access generated header file."
    exit 0
}

# Write to rc file if the content has changed or if the file doesnt exist
try {
    if (!(Test-Path -Path $generatedFilesFolder\$generatedRCFileName) -or (($rcFileContent + "`r`n") -ne (Get-Content $generatedFilesFolder\$generatedRCFileName -Raw))) {
        Set-Content -Path $generatedFilesFolder\$generatedRCFileName -Value $rcFileContent -Encoding "utf8"
    }
    else {
        # echo "Skipping write to generated rc file"
    }
}
catch {
    echo "Failed to access generated rc file."
    exit 0
}

# Write to rust file if the content has changed or if the file doesnt exist
try {
    if (!(Test-Path -Path $generatedFilesFolder\$generatedRustFileName) -or (($rustFileContent + "`r`n") -ne (Get-Content $generatedFilesFolder\$generatedRustFileName -Raw))) {
        Set-Content -Path $generatedFilesFolder\$generatedRustFileName -Value $rustFileContent  -Encoding "utf8"
    }
    else {
        # echo "Skipping write to generated header file"
    }
}
catch {
    echo "Failed to access generated rust file."
    exit 0
}
