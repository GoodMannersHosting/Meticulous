export const WEBHOOK_HELP_BASH = [
	'BASE_URL="https://your-meticulous-host"',
	'ORG_ID="your_org_id"',
	'TRIGGER_ID="your_trigger_id"',
	'SECRET="your_webhook_secret"',
	'',
	'BODY=\'{"event":"push","branch":"main"}\'',
	'SIG="sha256=$(printf \'%s\' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" | sed \'s/.*= //\' | tr \'[:upper:]\' \'[:lower:]\')"',
	'',
	'curl -sS -X POST "$BASE_URL/api/v1/webhooks/$ORG_ID/$TRIGGER_ID" \\',
	'  -H "Content-Type: application/json" \\',
	'  -H "X-Hub-Signature-256: $SIG" \\',
	'  --data-binary "$BODY"'
].join('\n');

export const WEBHOOK_HELP_PWSH = [
	'$BaseUrl = "https://your-meticulous-host"',
	'$OrgId = "your_org_id"',
	'$TriggerId = "your_trigger_id"',
	'$Secret = "your_webhook_secret"',
	'$Body = \'{"event":"push","branch":"main"}\'',
	'',
	'$hmac = New-Object System.Security.Cryptography.HMACSHA256',
	'$hmac.Key = [Text.Encoding]::UTF8.GetBytes($Secret)',
	'$hash = $hmac.ComputeHash([Text.Encoding]::UTF8.GetBytes($Body))',
	'$hex = -join ($hash | ForEach-Object { $_.ToString("x2") })',
	'$Sig = "sha256=$hex"',
	'',
	'Invoke-RestMethod `',
	'  -Uri "$BaseUrl/api/v1/webhooks/$OrgId/$TriggerId" `',
	'  -Method Post `',
	'  -Body $Body `',
	'  -ContentType "application/json" `',
	'  -Headers @{ "X-Hub-Signature-256" = $Sig }'
].join('\n');

export const WEBHOOK_HELP_PYTHON = [
	'import hashlib, hmac, urllib.request',
	'',
	'base_url = "https://your-meticulous-host"',
	'org_id = "your_org_id"',
	'trigger_id = "your_trigger_id"',
	'secret = b"your_webhook_secret"',
	'body = b\'{"event":"push","branch":"main"}\'',
	'',
	'sig = "sha256=" + hmac.new(secret, body, hashlib.sha256).hexdigest()',
	'url = f"{base_url.rstrip(\'/\')}/api/v1/webhooks/{org_id}/{trigger_id}"',
	'req = urllib.request.Request(',
	'    url,',
	'    data=body,',
	'    method="POST",',
	'    headers={',
	'        "Content-Type": "application/json",',
	'        "X-Hub-Signature-256": sig,',
	'    },',
	')',
	'with urllib.request.urlopen(req) as resp:',
	'    print(resp.read().decode())'
].join('\n');
