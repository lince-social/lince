**Lince Android signing and Google Play**

Prepared on 2026-09-28. This guide covers getting a signed build onto your phone and later publishing it through Google Play. Product changes and Karma are outside this work. Existing debug/test installations can be discarded; no migration of their data is required.

The owner has reported adding the eight GitHub signing secrets. Successful release builds still need verification. Play Store setup is currently deferred: use directly installed signed APKs without creating a Play account or paying its fee. When Play work resumes, start with its internal testing track for the owner's phone; public release is a separate later step. The remaining engineering and owner tasks are in [MOBILE_PLAN.md](MOBILE_PLAN.md).

**1. What the files and keys mean**

| Term | Meaning for Lince |
| --- | --- |
| APK | The file Android installs. Download this onto your phone for direct installation. |
| AAB, or Android App Bundle | The release file uploaded to Play. Google turns it into APKs for users' devices. You do not install an AAB by tapping it. |
| Package/application ID | Android's name for the app: `social.lince.mobile` for the release. It is different from the visible name “Lince.” |
| Private key | A secret used to create signatures. It must stay with the publisher and authorized signing systems. |
| Public certificate | Contains the corresponding public key. Android can use it to check a signature. Sharing this certificate is safe. |
| Certificate fingerprint | A short SHA-256 identifier for a public certificate. Compare it to confirm which signing identity was used. |
| Keystore | A password-protected file holding a private key and its certificate. Our files use PKCS12 and end in `.p12`. |
| Alias | The name of a key inside its keystore. It is not a password. |
| Version code | An integer that increases with releases so Android and Play can order updates. |
| Version name | The human-readable version, currently taken from the Rust workspace. |

Signing connects the APK to its publisher's key and detects changes after signing. It establishes continuity between releases. The connection between that key and “official Lince” comes from Lince's trusted distribution channels and the publisher's identity. These keys do not encrypt Records and are separate from Organ and Cell keys. [Android signing concepts](https://developer.android.com/studio/publish/app-signing).

The normal route will be:

```text
Direct download:
Lince build -> app-signing key -> signed APK -> your phone

Google Play:
Lince build -> upload key -> signed AAB -> Google Play
Google Play -> app-signing key -> signed APKs -> users' phones
```

Google checks the upload signature to recognize submissions from you. The phone checks the app signature. With Play App Signing, Google holds the app key used for Play delivery. We will supply our own app key so independently built APKs can use that same identity. The upload key remains separate and can be replaced through Play's recovery process. [Play App Signing](https://support.google.com/googleplay/android-developer/answer/9842756?hl=en).

**2. Your two keys have been created**

Their private directory is:

```text
/home/user/git/.lince-android-signing/
```

This is outside the Lince repository and on the persistent `/home/user/git` filesystem. Its permissions are `0700`; generated files are `0600`. Both keys are RSA-4096 with SHA256withRSA certificates valid for 10,950 days, approximately 30 years. Each key has a different randomly generated password. They have been tested by signing and verifying separate sample archives.

| Purpose | Private keystore | Alias | Password file |
| --- | --- | --- | --- |
| Sign directly installed APKs; later supply to Play App Signing | `app-signing.p12` | `lince-app` | `app-signing.password` |
| Sign bundles submitted to Play | `play-upload.p12` | `lince-upload` | `play-upload.password` |

All paths in this table are relative to the private directory above. Within each keystore, the store password and key password are the same. The two keystores use different passwords.

Each also has a public `.pem` certificate and a public `.der` certificate. These are two encodings of the same certificate, not additional keys. `public-certificates.json` records their public details.

| Certificate | SHA-256 fingerprint |
| --- | --- |
| App signing | `E3:45:70:81:EA:3A:07:E6:EC:AF:43:A2:48:1F:C6:9E:6C:8C:A2:12:B4:11:F3:60:94:65:E1:DC:F6:EB:98:27` |
| Play upload | `52:E8:3C:80:10:E2:D4:61:18:93:C3:CF:B0:3F:91:C2:2A:BD:54:B3:5B:FD:4C:35:35:AD:6C:68:0C:12:9E:F2` |

Do not generate new keys for each release. Reuse these files. A `.pem` certificate cannot replace a lost private keystore.

**3. Where to keep them**

1. Keep the working keystores in the private directory above. Never put `.p12` files or their passwords in the Git repository, an issue, a chat message or a public release artifact.
2. Import each password into your password manager with the corresponding filename and alias.
3. Make an independent encrypted backup of both keystores and the public certificate information, such as on an external drive. Keep the passwords recoverable separately. One copy on this laptop is not an independent backup.
4. Verify that the backup can be opened before relying on it. The `.password` files are convenient local plaintext copies protected by filesystem permissions. Anyone who can read both the keystore and its password can use that key.
5. Give GitHub Actions access through private repository secrets, using the instructions below. GitHub secrets are build credentials, not your only backup.
6. Later, supply the app key to Google only through Play Console's encrypted key-transfer procedure. Supply the upload **public certificate** when Play asks to register the separate upload key.

No private keys or passwords are included in this document. The owner reports configuring GitHub secrets; transfer to Google and successful signing in CI are not yet verified.

**4. Configure GitHub Actions secrets**

In the `lince-social/lince` repository, open Settings, then Secrets and variables, then Actions. Add the following repository secrets. You can instead run the commands below with your already authenticated `gh` CLI. [GitHub secret setup](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets).

| Secret | Value |
| --- | --- |
| `LINCE_ANDROID_KEYSTORE_BASE64` | Base64 encoding of `app-signing.p12` |
| `LINCE_ANDROID_STORE_PASSWORD` | Contents of `app-signing.password` |
| `LINCE_ANDROID_KEY_ALIAS` | `lince-app` |
| `LINCE_ANDROID_KEY_PASSWORD` | Contents of `app-signing.password` |
| `LINCE_ANDROID_UPLOAD_KEYSTORE_BASE64` | Base64 encoding of `play-upload.p12` |
| `LINCE_ANDROID_UPLOAD_STORE_PASSWORD` | Contents of `play-upload.password` |
| `LINCE_ANDROID_UPLOAD_KEY_ALIAS` | `lince-upload` |
| `LINCE_ANDROID_UPLOAD_KEY_PASSWORD` | Contents of `play-upload.password` |

Base64 makes the binary file usable as a text secret. It is not encryption. The commands pipe values directly into GitHub rather than printing them. Do not enable shell tracing while handling these files.

```sh
LINCE_SIGNING_DIR=/home/user/git/.lince-android-signing

base64 -w 0 "$LINCE_SIGNING_DIR/app-signing.p12" | gh secret set LINCE_ANDROID_KEYSTORE_BASE64 --repo lince-social/lince
tr -d '\r\n' < "$LINCE_SIGNING_DIR/app-signing.password" | gh secret set LINCE_ANDROID_STORE_PASSWORD --repo lince-social/lince
gh secret set LINCE_ANDROID_KEY_ALIAS --repo lince-social/lince --body lince-app
tr -d '\r\n' < "$LINCE_SIGNING_DIR/app-signing.password" | gh secret set LINCE_ANDROID_KEY_PASSWORD --repo lince-social/lince

base64 -w 0 "$LINCE_SIGNING_DIR/play-upload.p12" | gh secret set LINCE_ANDROID_UPLOAD_KEYSTORE_BASE64 --repo lince-social/lince
tr -d '\r\n' < "$LINCE_SIGNING_DIR/play-upload.password" | gh secret set LINCE_ANDROID_UPLOAD_STORE_PASSWORD --repo lince-social/lince
gh secret set LINCE_ANDROID_UPLOAD_KEY_ALIAS --repo lince-social/lince --body lince-upload
tr -d '\r\n' < "$LINCE_SIGNING_DIR/play-upload.password" | gh secret set LINCE_ANDROID_UPLOAD_KEY_PASSWORD --repo lince-social/lince

gh secret list --repo lince-social/lince
```

These commands require the Linux `base64` utility and GitHub CLI. The last command lists names, not secret contents. Existing values with the same names will be replaced, so use these commands for initial setup or an intentional credential update.

The workflow injects app credentials only into the APK signing step and upload credentials only into the bundle signing step. Both signing options are restricted to manually triggered builds from `main`. Pull-request checks do not receive these keys. Keep control of who can change and run workflows on `main`; a workflow with signing credentials can use them.

**5. Get the next signed build onto your phone**

The workflow changes must first be reviewed and pushed to `main`; creating this guide has not pushed the shared working tree.

1. Verify the app-signing secrets already configured from section 4 through a successful build; do not regenerate the keys.
2. Open the repository's Actions tab and select `lince-android`.
3. Choose Run workflow, select `main`, enable `signed_release`, and leave `play_bundle` disabled for now.
4. Wait for a successful run. Download the artifact named `lince-android-arm64-release`.
5. Extract the download and copy `app-release.apk` to the Moto G05. The `.sha256` file is a checksum, not an application.
6. Open the APK on the phone. If prompted, allow that browser or file manager to install apps from this source.
7. For later builds, repeat these steps and install over the previous release. No uninstall should be needed when the package, signing identity and version ordering are correct.

The release package is `social.lince.mobile`. The old debug package is `social.lince.mobile.debug`, and automated smoke tests use `social.lince.mobile.smoketest`. You have said these old installations contain no data to preserve, so no migration is needed. You may remove them to avoid several similar icons.

The workflow sets `versionCode` to `GITHUB_RUN_NUMBER * 100 + GITHUB_RUN_ATTEMPT`. Keep that sequence increasing across releases. If the workflow is replaced or its numbering resets, choose a value higher than every release already installed or submitted to Play.

You can request the same build from a terminal:

```sh
gh workflow run android.yml --repo lince-social/lince --ref main -f signed_release=true -f play_bundle=false
gh run list --repo lince-social/lince --workflow android.yml --limit 5
```

To verify a downloaded APK with the installed Android tools:

```sh
"$ANDROID_HOME/build-tools/35.0.0/apksigner" verify --print-certs app-release.apk
sha256sum -c app-release.apk.sha256
```

On NixOS, if the SDK script's `/bin/bash` interpreter is unavailable, invoke it with `bash "$ANDROID_HOME/build-tools/35.0.0/apksigner"` and the same arguments. Compare the printed certificate SHA-256 with the app-signing fingerprint in section 2. The APK checksum checks the downloaded file; the certificate identifies its signer.

**6. Open a Google Play publisher account**

Use [Google Play Console](https://play.google.com/console/), not the separate Android Developer Console intended for distribution outside Play.

1. Choose a Google account you will retain as Lince's publisher and enable two-step verification.
2. Register for Play Console, accept its terms and pay the US$25 one-time registration fee. This pays for the publisher account; key generation itself has no fee.
3. Complete identity and contact verification. New personal accounts also require verification of access to an Android device using the Play Console app. [Account registration](https://support.google.com/googleplay/android-developer/answer/6112435?hl=en-AU).
4. Choose personal ownership if you are publishing as yourself, or organization ownership if a real organization will own the listing. A Google organization account is unrelated to a Lince Organ.

Organization accounts generally require a D-U-N-S business identifier and verified organization details. Personal accounts also involve verified legal details; the visible developer name does not make the account anonymous. Review which details Google displays before registering. [Required identity information](https://support.google.com/googleplay/android-developer/answer/13628312?hl=en).

After verification, choose Home > Create app. Enter `Lince`, choose the default listing language, select App, provide a support email, choose free or paid distribution, and complete the declarations. The publisher registration fee is separate from whether users pay for Lince. Our release bundle identifies the app as `social.lince.mobile`; keep this package for future updates. [Create an app](https://support.google.com/googleplay/android-developer/answer/9859152).

**7. Register Lince's signing identity with Play**

Finish this before rolling out the first internal release so later installs and updates use the intended identity.

1. Create the app entry in Play Console and start its release setup.
2. Find Play App Signing, currently under Protected with Play > Play Store distribution > Go to Play app signing; the first-release flow can also expose this under App integrity. If Google has selected a generated key, use Change the app signing key before rolling out the release.
3. Choose to provide an existing app-signing key. Do not retain a different Google-generated key if the goal is to share the signing identity with our independently built APKs.
4. Use the PEPK tool and the exact encryption instructions supplied by that Play Console page. Select `app-signing.p12`, alias `lince-app`, and its password. PEPK creates an encrypted export for Google; upload that export through the console. Do not upload a raw keystore to the store listing or an artifact download.
5. Register `play-upload.pem` as the separate upload certificate when prompted. The upload private key stays with your build systems; Google needs its certificate to recognize submissions.
6. Compare Play's app-signing and upload fingerprints with section 2. Resolve any difference before distributing a release.

Play defaults and menu names can change. Follow the current console's existing-key instructions. Retain your own backup: Play does not offer the uploaded private key back for download. [Existing-key setup](https://developer.android.com/studio/publish/app-signing).

**8. Prepare a bundle and the store listing**

Before submission, Lince still needs its Android target raised and validated: the project currently targets API 35, while new Play submissions require API 36 or higher from August 31, 2026. Raising the target does not by itself mean dropping support for your Android 15 phone. [Target API policy](https://support.google.com/googleplay/android-developer/answer/11926878?hl=en).

The workflow now has a `play_bundle` option. After configuring the upload secrets, enabling it generates the artifact `lince-android-arm64-play-bundle`, containing `app-release.aab` and its checksum. You may request both signing options in one run. APKs use the app key; bundles use the upload key. Local packaging selects upload credentials with `-PlinceSigning=upload bundleRelease`; `assembleRelease` rejects that choice to prevent distributing APKs with the upload identity.

A generated AAB is not evidence of Play eligibility. Complete the API upgrade and validate the resulting bundle before submission. Full release APK/AAB builds with the new permanent keys and a GitHub workflow run remain to be verified.

Validate the bundle's generated APKs and native libraries for 16 KB page sizes, retain native crash symbols, and check the download size and compatibility reported by Play. The existing emulator job is one check, not proof that the final release bundle passes all these checks. [Native-library requirements](https://developer.android.com/guide/practices/page-sizes).

For the listing and review, prepare:

- App name, description, icon, screenshots, category and support email.
- A reachable privacy policy describing actual local storage, pairing and data exchange.
- Accurate Data safety answers based on the complete app and its dependencies. “No central server” alone does not answer how data leaves a device.
- Content rating, target audience and ads declarations.
- Clear reviewer instructions to create a disposable Organ and reach the relevant screens. Provide test access where authentication would otherwise block review.

Use Play Console's current checklist to identify additional declarations that apply. [Review preparation](https://support.google.com/googleplay/android-developer/answer/9859455?hl=en).

**9. Test through Play before publishing publicly**

Start with Test and release > Testing > Internal testing. In Testers, create/select an email list containing the Google account used by Play Store on your Moto, add a feedback contact, and save it. In Releases, create a release, upload `app-release.aab`, finish the signing setup above, add release notes, and resolve any blocking errors before rollout. [Release steps](https://support.google.com/googleplay/android-developer/answer/9859348?hl=en).

Open the tester opt-in link on the phone using that same account, join the test and follow its Play Store install link. Searching for Lince will not find an internal-only release. Internal testing allows up to 100 testers and can start before the full listing is finished; availability can take time after rollout. Use Internal testing, not Internal app sharing, for this update workflow. [Internal-test setup](https://support.google.com/googleplay/android-developer/answer/9845334?hl=en).

If the publisher is a personal account created after November 13, 2023, public release requires a closed test with at least 12 testers continuously opted in for 14 days. After that, apply for production access and describe the testing. Internal testing on your phone does not satisfy this closed-test requirement, and completing 14 days does not guarantee approval. [Personal-account testing](https://support.google.com/googleplay/android-developer/answer/14151465?hl=en).

Once approved and the app meets the submission requirements, select the countries, submit the production release and follow the review process. Building an AAB or uploading GitHub secrets does not publish the app.

For development in Brazil, ADB installation remains available without Android developer verification. Review the distribution verification rules when opening public distribution; Play Console can cover distribution on and outside Play. [Developer verification FAQ](https://developer.android.com/developer-verification/guides/faq).

**10. Send each following update to the phone**

1. Get the reviewed code onto `main` and run `lince-android` with `play_bundle=true`. Enable `signed_release` too only when you also want a directly installed APK. Download and extract `lince-android-arm64-play-bundle`; the upload is its `app-release.aab`, not the artifact ZIP or APK.
2. Keep the same app entry, package and signing setup. Confirm that the new version code exceeds the version installed on the phone and every earlier submitted version. CI currently calculates it as `GITHUB_RUN_NUMBER * 100 + GITHUB_RUN_ATTEMPT`; a new workflow with reset numbering needs an explicit continuation of that sequence. Android uses this number for update ordering. [Versioning](https://developer.android.com/studio/publish/versioning).
3. Create another release on the Internal testing track, upload the new AAB, enter release notes and complete the rollout. The GitHub artifact alone does not send an update to Play. [Release steps](https://support.google.com/googleplay/android-developer/answer/9859348?hl=en).
4. On the phone, open Lince's Play Store details page and choose Update when available. For automatic updates, enable More > Enable auto update on that page; global network preferences determine whether updates use Wi-Fi or mobile data. Updates need not appear immediately. [Phone update settings](https://support.google.com/googleplay/answer/113412?hl=en).
5. Keep an existing disposable Record and draft across the update, then verify Organ identity, Cell keys, selected profile and synchronization. Do not uninstall between these release-update checks.

If Play offers no update, check the tester account and opt-in, release status, device compatibility and version code. A newer APK installed directly may already have a higher code than the Play release. Upload a later build instead of expecting Play to downgrade it. A signing mismatch needs investigation; regenerating the keys does not fix update continuity.

**11. Optional automatic upload from GitHub**

This is not implemented or needed for the manual steps above. Signing secrets let CI sign files; they do not grant permission to publish to Play.

After the first manual release works, automation can use a Google Cloud project with the Google Play Developer API enabled and a service account invited through Play Console's Users and permissions. Give it access only to Lince and the testing-release permissions it needs. Configure CI authentication separately, then add an explicit internal-release upload step. Production rollout should remain a separate choice. [Google API setup](https://developers.google.com/android-publisher/getting_started).

**12. What remains after configuring GitHub secrets**

- Make and verify an independent private backup, and store the passwords in your password manager.
- Verify the configured signing secrets through CI and get the reviewed workflow changes onto `main`.
- Run the signed APK workflow and verify two successive releases install as an update.
- Raise and test the Android target for current Play requirements, then verify the signed AAB pipeline.
- Create the Play account, supply the app key through PEPK, register the upload certificate and complete the listing/testing steps.

If an upload key is lost or compromised, use Play Console's upload-key reset process. If the app key is lost, your independent backup is needed for direct APK signing; if it is compromised, coordinate a supported signing-key upgrade rather than silently replacing it. [Key recovery](https://support.google.com/googleplay/android-developer/answer/9842756?hl=en).
