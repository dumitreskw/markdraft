# Signing MarkDraft releases

Unsigned builds work, but users see warnings: Windows SmartScreen says
"Windows protected your PC", and macOS refuses to open the app until the user
overrides Gatekeeper. Signing removes most of that friction.

The release workflow (`.github/workflows/release.yml`) signs automatically as
soon as the secrets below exist. Until then it keeps producing unsigned builds.

| Platform | What you need | Cost | Time to set up |
|---|---|---|---|
| macOS | Apple Developer Program membership | 99 USD / year | 1–2 days (organisations: up to 2 weeks) |
| Windows | Azure Artifact Signing, **or** an OV certificate from a CA | about 10 USD / month, or about 100–350 USD / year | 1–20 business days for identity checks |
| Linux | Nothing required | — | — |

---

## 0. Free option while MarkDraft is not sold yet

A *TLS/SSL* certificate (for example from Let's Encrypt) cannot sign
software: code signing needs a certificate issued for code signing. You can,
however, make your own **self-signed** code-signing certificate for free.

What it does and doesn't do:

| | Self-signed (free) | Trusted (Azure / CA / Apple) |
|---|---|---|
| Proves the installer was not modified after you built it | Yes | Yes |
| Windows shows your name as publisher | Only on machines that trust your certificate | Everywhere |
| Removes "Windows protected your PC" (SmartScreen) for other people | **No** | Builds up over time |
| macOS opens without a Gatekeeper warning | **No** (ad-hoc signing only) | Yes, once notarised |

So it is useful for you and your team or testers, not for the public.

**Windows (2 minutes):**

1. Run `pwsh scripts/make-test-certificate.ps1` and choose a password.
2. Add the two GitHub secrets it prints: `WINDOWS_TEST_CERTIFICATE` and
   `WINDOWS_TEST_CERTIFICATE_PASSWORD`. Releases are then signed with it
   automatically (until Azure secrets exist, which take priority).
3. On each test machine, import `markdraft-test.cer` with the two commands the
   script prints. Those machines then show "MarkDraft (test signing)" as the
   publisher instead of "Unknown publisher".

**macOS (nothing to do):** releases without Apple secrets are signed *ad
hoc*. Apple Silicon Macs then show "unidentified developer" instead of "app is
damaged". Users open it once with right-click → **Open** (or System Settings →
Privacy & Security → **Open Anyway**).

---

## 1. macOS: Developer ID + notarisation

### 1.1 Join the Apple Developer Program

1. Go to <https://developer.apple.com/programs/enroll/> and sign in with an
   Apple Account that has two-factor authentication turned on.
2. Enroll as an **Individual** (your own name appears as the developer) or as an
   **Organization**. An organisation needs a D-U-N-S number, which is free
   from <https://developer.apple.com/enroll/duns-lookup/> but can take up to two weeks.
3. Pay the 99 USD yearly fee and wait for the confirmation email.

### 1.2 Create the "Developer ID Application" certificate

You can do this with or without a Mac.

**With a Mac**

1. Open **Keychain Access** → menu **Keychain Access → Certificate Assistant →
   Request a Certificate From a Certificate Authority**. Enter your email, choose
   **Saved to disk**, and save the `.certSigningRequest` file.
2. Go to <https://developer.apple.com/account/resources/certificates/add>,
   choose **Developer ID Application**, and upload the request file.
   Only the Account Holder can create this certificate type.
3. Download the `.cer` file and double-click it to add it to your keychain.
4. In Keychain Access → **login** keychain → **My Certificates**, expand the
   certificate, right-click it → **Export**, choose **Personal Information
   Exchange (.p12)**, and set a strong password.

**Without a Mac (Windows, with OpenSSL, for example from Git Bash)**

```sh
openssl genrsa -out developerid.key 2048
openssl req -new -key developerid.key -out developerid.csr \
  -subj "/emailAddress=you@example.com/CN=Your Name/C=XX"   # XX = your two-letter country code
```

Upload `developerid.csr` as in step 2 above, download `developerID_application.cer`, then:

```sh
openssl x509 -inform DER -in developerID_application.cer -out developerid.pem
openssl pkcs12 -export -inkey developerid.key -in developerid.pem \
  -out developerid.p12 -name "Developer ID Application"
```

Keep `developerid.key` and the `.p12` file somewhere safe and private. Anyone
with them can sign software as you.

### 1.3 Collect the values the workflow needs

1. **Certificate as Base64**:
   `openssl base64 -A -in developerid.p12 -out certificate-base64.txt`
2. **Signing identity**: the certificate's full name, for example
   `Developer ID Application: Your Name (ABCDE12345)`. It is shown on the
   certificate in the developer portal, or by `security find-identity -v -p codesigning` on a Mac.
3. **Team ID**: the 10-character ID at <https://developer.apple.com/account> → **Membership details**.
4. **App-specific password** for notarisation: <https://account.apple.com> →
   **Sign-In and Security** → **App-Specific Passwords** → generate one named
   "MarkDraft notarisation".

### 1.4 Add them to GitHub

In the repository: **Settings → Secrets and variables → Actions → New repository secret**:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | contents of `certificate-base64.txt` |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (ABCDE12345)` |
| `APPLE_ID` | your Apple Account email |
| `APPLE_PASSWORD` | the app-specific password |
| `APPLE_TEAM_ID` | the Team ID |

That's all for macOS. The next tagged release is signed and notarised.

---

## 2. Windows: pick one route

Both routes need your identity to be verified. Pick based on who you are:

| You are… | Use |
|---|---|
| A **company** registered in the EU, UK, US, Canada, Australia, New Zealand, Japan, South Korea, Singapore, Switzerland, Norway or Israel | **Route A: Azure Artifact Signing** (cheapest, works in CI out of the box) |
| An **individual** in the US or Canada | **Route A** |
| An **individual elsewhere** without a company | **Route B: OV certificate from a certificate authority** |

Since 2024, signed apps still build SmartScreen reputation over time, so the
first releases may still show a warning. EV certificates no longer skip this.

### Route A: Azure Artifact Signing

About 9.99 USD/month (Basic: 5,000 signatures/month).

1. **Azure account**: create one at <https://azure.microsoft.com> and add a
   pay-as-you-go **subscription**. As an individual, the subscription's billing
   account must be of type *Individual*, and its legal name and address must
   match your ID.
2. **Register the provider**: Azure portal → **Subscriptions** → your
   subscription → **Resource providers** → select **Microsoft.CodeSigning** →
   **Register**.
3. **Create the account**: search **Artifact Signing Accounts** → **Create** →
   new resource group `markdraft-signing`, account name e.g. `markdraftsign`,
   region **West Europe** (or the one nearest you), pricing **Basic** →
   **Review + Create**. Note the region's endpoint, e.g.
   `https://weu.codesigning.azure.net` (North Europe: `neu`, Poland Central: `plc`, East US: `eus`).
4. **Give yourself the verifier role**: in the account → **Access control (IAM)**
   → **Add role assignment** → **Artifact Signing Identity Verifier** → your user.
5. **Identity validation**: in the account → **Identity validations** →
   **New identity** → **Public**:
   - *Organization*: legal company name, website, an email on the company's
     domain, business registration number, address. Then you verify yourself
     with an ID document through the link you're sent.
   - *Individual* (US/Canada): the form fills in from your billing account; then
     verify with your phone and ID document (AU10TIX + Microsoft Authenticator).
   This takes 1–20 business days. Wait for status **Completed**.
6. **Certificate profile**: in the account → **Certificate profiles** → **Create**
   → **Public Trust** → name `markdraft` → choose your completed identity validation → **Create**.
7. **A login for GitHub Actions** (service principal):
   1. Portal → **Microsoft Entra ID** → **App registrations** → **New registration**
      → name `markdraft-github-signing` → **Register**. Copy the
      **Application (client) ID** and **Directory (tenant) ID**.
   2. **Certificates & secrets** → **New client secret** → copy the secret **Value**
      (shown only once). Put a reminder in your calendar before it expires.
   3. Back in the Artifact Signing account → **Access control (IAM)** → **Add role
      assignment** → **Artifact Signing Certificate Profile Signer** → **Select
      members** → search `markdraft-github-signing` → **Review + assign**.
      Allow about five minutes for the role to take effect.
8. **GitHub**: **Settings → Secrets and variables → Actions**:

   | Kind | Name | Value |
   |---|---|---|
   | Secret | `AZURE_CLIENT_ID` | Application (client) ID |
   | Secret | `AZURE_CLIENT_SECRET` | the client secret value |
   | Secret | `AZURE_TENANT_ID` | Directory (tenant) ID |
   | Variable | `AZURE_SIGNING_ENDPOINT` | e.g. `https://weu.codesigning.azure.net` |
   | Variable | `AZURE_SIGNING_ACCOUNT` | e.g. `markdraftsign` |
   | Variable | `AZURE_SIGNING_PROFILE` | e.g. `markdraft` |

   The workflow installs `artifact-signing-cli` and signs the app, the MSI and
   the NSIS installer.

### Route B: OV code-signing certificate

For individuals outside the US and Canada. Since June 2023 the private key must
live in hardware or a cloud HSM, and from March 2026 certificates last at most
458 days (about 15 months).

1. Choose a CA that issues certificates to individuals in your country **and**
   offers cloud signing usable from CI, for example **Certum** (SimplySign) or
   **SSL.com** (eSigner). Ask the CA before paying whether it issues to
   individuals without a registered business in your country.
2. Order an **OV / Individual Validation code signing** certificate with
   **cloud signing** (not a USB token: a token cannot be used from GitHub's
   servers).
3. Complete the identity check (ID document, sometimes a video call or
   notarised form).
4. Follow the CA's CI instructions to get the credentials for its signing tool
   (SSL.com: username, password, credential ID, TOTP secret; Certum: SimplySign
   account). Store them as GitHub secrets.
5. Tell me which CA you chose, and I'll replace the Windows signing step in
   `release.yml` with that CA's tool (Tauri runs any `signCommand`).

---

## 3. Linux

No signing is required for AppImage, `.deb` or `.rpm` downloads. If you later
publish an APT/RPM repository or Flathub listing, those have their own
signing, set up at that time.

## 4. Publish a release and check it

1. Bump the version in `src-tauri/tauri.conf.json` and the root `Cargo.toml`.
2. Tag and push:
   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. GitHub → **Actions** → watch **Release** (about 30 minutes). When it
   finishes, a **pre-release** appears under **Releases** with every
   package (Windows x64/x86/ARM64 installers, MSIs and portable zips; macOS
   universal, Apple silicon and Intel DMGs; Linux x64/ARM64 AppImage, deb
   and rpm; the headless CLI) and `SHA256SUMS.txt`.
   It is published only when every build succeeded; otherwise it stays a
   draft (visible only to you) until you fix the build and re-run it.

   To try the whole pipeline without a release, push a branch named
   `release-test/<anything>`: the same files are kept as workflow artifacts
   for 14 days.
4. Check the signatures:
   - Windows (PowerShell): `Get-AuthenticodeSignature .\MarkDraft-0.1.0-windows-x64-setup.exe`
     should say `Valid`.
   - macOS: `codesign --verify --deep --strict -v /Applications/MarkDraft.app` and
     `spctl -a -vv /Applications/MarkDraft.app` should say `accepted, source=Notarized Developer ID`.
5. Edit the release notes if you like. When a signed version is ready for
   everyone, untick **Set as a pre-release** so it becomes the latest release.

## 5. In-app updates (free, needed for every release)

Installed copies of MarkDraft check `latest.json` on the `updates` branch and
only install files signed with MarkDraft's **updater key**. That key is not a
code-signing certificate: it is a free key pair made by the Tauri CLI.

1. Make the key pair once (already done if `~/.tauri/markdraft-updater.key` exists):
   `pnpm tauri signer generate -w ~/.tauri/markdraft-updater.key`
   The public half is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.
2. Add two repository secrets (**Settings → Secrets and variables → Actions**):
   - `TAURI_SIGNING_PRIVATE_KEY`: the contents of `markdraft-updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password
3. Tag a release as usual. Each build then signs its setup.exe, MSI, macOS
   `.app.tar.gz` and AppImage, and once the release is published the
   **Update feed** job writes `latest.json` to the `updates` branch.

Without the secrets, releases build as before and the feed is left alone.
Copies older than the first release with an updater (0.1.0) must be updated
by hand once.

## What to keep safe

- **The updater key and its password.** If they are lost, installed copies can
  never update themselves again (they only trust this key); if leaked, rotate
  it and ship the new public key in a release signed with the old one.
- The `.p12` file and its password, the Apple app-specific password.
- The Azure client secret (rotate before it expires; update the GitHub secret).
- Never commit any of these; they live only in GitHub secrets and your password manager.
