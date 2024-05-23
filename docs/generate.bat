@echo off

@rem calculate the next version number based on whatever the last draft-xyz.docx file is
@rem this is a bit of a hack, but it works

@rem get the last draft file
for /f "delims=" %%a in ('dir /b /on draft-*.docx') do set lastdraft=%%a

@rem if we didn't find an existing one, start with 000
if "%lastdraft%"=="" set lastdraft=draft-000.docx

@rem get the version number from the last draft file
for /f "tokens=2 delims=-." %%a in ("%lastdraft%") do set /a version=%%a+1

echo Generating draft-%version%.docx...

@rem create the new draft file
@rem
@rem mermaid-filter.cmd is from github.com/raghur/mermaid-filter. That's deeply
@rem out of date, so some of the newer features are missing. You can manually
@rem patch the mermaid.min.js if you want though.
pandoc -s -F mermaid-filter.cmd  --from=markdown+yaml_metadata_block --to=docx .\draft.md -o .\draft-%version%.docx

@rem delete mermaid-filter.err, if it's empty

if exist mermaid-filter.err (
    for %%a in (mermaid-filter.err) do if %%~za==0 del mermaid-filter.err
)
