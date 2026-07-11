Set-Location -LiteralPath $PSScriptRoot
npm run dev *>&1 | Tee-Object -FilePath 'dev-run.log'
