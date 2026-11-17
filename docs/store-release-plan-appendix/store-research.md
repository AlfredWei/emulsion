# Selling Emulsion through the Mac App Store and the Microsoft Store

## Goal

The owner wants to sell Emulsion as a paid app through the Mac App Store and the Microsoft Store. Emulsion is a local-first desktop photo editor and catalog that replaces Lightroom. It is built on Tauri 2 with a Rust backend and a wgpu GPU pipeline. It bundles about 500 MB of ONNX models and a sidecar binary. It has an optional export-plugin hook that runs external programs. It stores face groups locally. It can fetch online map tiles and run place search. The owner needs an accurate plan. This report collects the store rules that shape that plan.

## How to read the citations

Every source was retrieved on 2026-11-17. Each citation gives the URL and the document's own date or version. "No date shown" means the page carries only a 2026 copyright footer. Claims marked **inference** are my reasoning from the cited text. They are not stated by the source. Claims I could not confirm in an official source are repeated in the last section.

---

## 1. Mac App Store

### 1.1 Tauri's own App Store path

Tauri publishes an official App Store guide (https://v2.tauri.app/distribute/app-store/, last updated 2026-08-20). It says the following.

- The App Store Connect bundle ID must match `identifier` in `tauri.conf.json`.
- An `Entitlements.plist` must enable `com.apple.security.app-sandbox` and set the application and team identifiers. It is wired in through `bundle > macOS > entitlements`. The app must work inside the sandbox.
- A "Mac App Store Connect" provisioning profile is embedded by mapping `embedded.provisionprofile` in `bundle > macOS > files`. The guide suggests a separate merged config file such as `tauri.appstore.conf.json` for this.
- An `Info.plist` must declare `ITSAppUsesNonExemptEncryption`.
- `bundle > category` must be set.
- Build with `universal-apple-darwin` for Apple silicon plus Intel. An Apple-silicon-only build sets `minimumSystemVersion` to `12.0`.
- Build the `.app`, wrap it with `xcrun productbuild` into a `.pkg` signed with a "Mac Installer Distribution" certificate, and upload with `xcrun altool --upload-app --type macos` using an App Store Connect API key.
- The guide does not mention sidecars, Transporter, the updater plugin, or In-App Purchase.

### 1.2 Sandbox, helper binaries and the sidecar

- App Sandbox is a requirement for App Store distribution (https://developer.apple.com/tutorials/data/documentation/security/protecting-user-data-with-app-sandbox.md, no date shown). A sandboxed app gets free access only to its container. It has no unrestricted access to the home folder.
- That page says an embedded command-line tool must inherit the containing app's sandbox.
- Apple's "Embedding a command-line tool in a sandboxed app" (https://developer.apple.com/tutorials/data/documentation/xcode/embedding-a-helper-tool-in-a-sandboxed-app.md, no date shown) gives the exact rules. The tool goes in `Contents/MacOS`. It must be signed with exactly two entitlements, `com.apple.security.app-sandbox` and `com.apple.security.inherit`. Apple warns that adding other entitlements can make the tool crash at launch. `get-task-allow` is incompatible with `inherit`, so the shipped tool cannot be debugged. An externally built tool is signed with its own identifier and hardened runtime, then re-signed on embed.
- Guideline 2.4.5(i) requires sandboxing and proper file-system use. 2.4.5(ii) requires a self-contained bundle with no code installed in shared locations (https://developer.apple.com/app-store/review/guidelines/, last updated 2026-10-16).

### 1.3 File access for a photo catalog

- The fetched Apple pages confirm only that sandboxed apps are confined to their container and must use proper APIs for data outside it (guidelines 2.4.5(i), 2.5.2, cited above).
- The entitlement keys a catalog would need are `com.apple.security.files.user-selected.read-write`, `com.apple.security.files.bookmarks.app-scope` and possibly `com.apple.security.assets.pictures.read-write`. Persistent access to folders the user picked uses security-scoped bookmarks. **I could not load Apple's file-access page body**, so these keys and the bookmark API are listed under "Uncertain".
- Map tiles and place search need outgoing network access. The usual key is `com.apple.security.network.client`. This is also unconfirmed from an official page this session.

### 1.4 Signing, certificates, notarisation, upload, TestFlight

- Certificate types (https://developer.apple.com/help/account/certificates/certificates-overview/, no date shown). "Apple Distribution" or "Mac App Distribution" signs the app for the store. "Mac Installer Distribution" signs the `.pkg`. "Developer ID Application" and "Developer ID Installer" are for distribution outside the store. Only the Account Holder or Admin can create distribution certificates.
- Hardened runtime is "not necessary for App Store apps" but is best practice (helper-tool page cited above). Notarisation is the Developer ID path. The notary service now requires `notarytool` or Xcode 14 or later (https://developer.apple.com/news/upcoming-requirements/, entry dated 2023-11-01).
- Since 2025-02-18 no file in a macOS bundle may carry the `com.apple.quarantine` attribute at upload (same page).
- Upload. Tauri documents `altool` (section 1.1). The Transporter app is not covered by any page I fetched.
- TestFlight supports macOS. Builds made with Xcode 13 or later can be tested. Up to 10,000 external and 100 internal testers. Builds expire after 90 days (https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/, no date shown).
- Upload SDK minimum. Apple lists Xcode 26 SDK minimums from 2026-04-28 and Xcode 27 SDK minimums from April 2027, but names no macOS SDK minimum (https://developer.apple.com/news/upcoming-requirements/ and https://developer.apple.com/app-store/submitting/, no date shown).
- Size. A macOS app may be up to 200 GB uncompressed (https://developer.apple.com/help/app-store-connect/reference/maximum-build-file-sizes/, no date shown). A 500 MB model bundle is far inside this.

### 1.5 Apple Developer Program

Source is https://developer.apple.com/programs/enroll/ (no date shown) unless noted.

- Cost is 99 USD per membership year, shown in local currency.
- Individuals enrol under their legal name, with two-factor authentication. P.O. boxes are refused.
- Organisations must be a legal entity. DBAs, trade names and branches are refused. A D-U-N-S number is required. The organisation needs a domain email and a working public website. The organisation name is shown as the seller.
- Timing. Apple says to allow up to 5 business days for D&B to issue a D-U-N-S number, then up to 2 business days for Apple to receive it (https://developer.apple.com/help/account/membership/D-U-N-S/, no date shown, via search summary). Apple publishes no total enrolment time. Forum reports range from 48 hours to several weeks (unofficial).

### 1.6 Pricing, trials and commission

- Guideline 3.1.1. Unlocking features or a full version must use In-App Purchase. Apps may not use their own unlock mechanisms "such as license keys" (guidelines, 2026-10-16).
- Guideline 2.4.5(vi). No license screens at launch, no license keys, no custom copy protection on the Mac App Store.
- Trials. A non-subscription app may offer a free time-limited trial as a Non-Consumable IAP at price tier 0 named "XX-day Trial". The app must state the duration, what stops working, and the price of full access before the trial starts (3.1.1). Subscriptions get trial offers through App Store Connect (3.1.2).
- So Emulsion can be a plain paid-upfront app. Or it can be free to download with a $0 trial IAP and a paid unlock IAP. Restorable purchases need a restore mechanism (3.1.1).
- Community StoreKit 2 plugins for Tauri exist. `tauri-plugin-iap` states macOS 13.0+ for StoreKit 2 (https://docs.rs/crate/tauri-plugin-iap/0.8.0). `tauri-plugin-apple-iap` 1.0.0 states macOS 12.0 (https://docs.rs/crate/tauri-plugin-apple-iap/latest). These are third-party, not Tauri official.
- Commission. The standard rate is 30% on digital goods. It is 15% under the Small Business Program (https://developer.apple.com/programs/whats-included/, no date shown, via search summary). Different rates may apply in Brazil, the EU, Japan, the Netherlands, Russia and South Korea (same page).
- Small Business Program (https://developer.apple.com/app-store/small-business-program/, no date shown). 15% on paid apps and IAP. Eligible if total proceeds across all associated accounts were at most USD 1 million in the prior calendar year, and new developers qualify. Crossing USD 1 million in the year reverts to the standard rate for the rest of that year. Enrolment is a separate step after accepting the Paid Apps Agreement.
- Tax and banking. The Account Holder must sign the Paid Apps Agreement before selling. Then tax forms (W-9, or W-8BEN / W-8BEN-E for non-US) and then bank details in the enrolled entity's name (https://developer.apple.com/help/app-store-connect/manage-agreements/sign-and-update-agreements/ and https://developer.apple.com/help/app-store-connect/manage-banking-information/enter-banking-information/, no date shown, via search summary).

### 1.7 Updates, downloaded code and data, external programs

- 2.4.5(vii). Mac App Store apps must update through the Mac App Store. A built-in self-updater is not allowed (guidelines, 2026-10-16).
- 2.4.5(iv). The app may not download or install standalone apps, kexts, code, or resources that add functionality or significantly change the reviewed app.
- 2.5.2. Apps must stay inside their bundle and container. They may not download, install or execute code that adds or changes features, including code from other apps.
- **Inference.** The export-plugin hook that runs arbitrary external programs conflicts with 2.5.2. A child process of a sandboxed app runs inside the app's sandbox (section 1.2), so most external tools would also fail at runtime. Plan to disable the hook in the store build.
- **Inference.** Bundling the 500 MB of ONNX models inside the app is the safe path. Downloading them after install risks 2.4.5(iv), because the models add features.
- Apple-hosted asset packs (Background Assets) exist with a 200 GB total and a 200-pack limit per app (https://developer.apple.com/help/app-store-connect/reference/app-uploads/apple-hosted-asset-pack-size-limits, no date shown). The page does not say whether macOS is supported. Unconfirmed.

### 1.8 Privacy, face data, age rating, export compliance

- 5.1.1. A privacy policy link is required in App Store Connect and inside the app. It must say what is collected, how, all uses, retention and deletion (guidelines, 2026-10-16).
- 5.1.2. Data from facial-mapping tools and the Camera and Photo APIs may not be used for marketing, advertising or use-based data mining. Sharing with third parties, including third-party AI, needs disclosure and explicit consent.
- 2.5.13 covers face recognition used for account authentication only. It does not apply to face grouping.
- Privacy labels (https://developer.apple.com/app-store/app-privacy-details/, no date shown). Data processed only on device is not "collected" and needs no disclosure. **Inference.** Local face grouping needs no label. Place-search queries and map-tile requests that send location text or coordinates to a server may need disclosure if the server retains them. That depends on the provider, so the provider's retention terms must be checked.
- Age rating. All apps moved to a new age-rating system. Developers had to answer the updated questionnaire by 2026-01-31 (https://developer.apple.com/news/upcoming-requirements/).
- Export compliance (https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance/, no date shown). Any app that uses encryption needs a determination. Setting the Info.plist key avoids the question on every submission. France controls encryption apps separately. The page does not list the HTTPS exemption explicitly.

---

## 2. Microsoft Store

### 2.1 Two submission paths

Microsoft accepts MSIX packages and plain MSI or EXE installers (https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/choose-distribution-path, 2026-10-07).

| | MSIX | MSI or EXE |
|---|---|---|
| Who signs | Microsoft re-signs, no certificate purchase | Publisher signs installer and every PE file with a cert chaining to the Microsoft Trusted Root Program |
| Who hosts | Microsoft CDN | Publisher hosts a versioned HTTPS URL that must not change |
| Updates | Store-managed | App or installer handles them |
| Commerce | MSIX docs list "commerce" as an MSIX benefit | See 2.4, store billing likely unavailable |

- MSI and EXE rules (Store Policy 10.2.9, https://learn.microsoft.com/en-us/windows/apps/publish/store-policies, version 7.20, 2026-09-14). Only `.msi` or `.exe`. Silent install, with UAC allowed. A standalone offline installer, not a downloader stub. PC only. A new binary needs a new versioned URL. The same rules appear at https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements (updated 2026-08-24).
- 10.4.4. Installers offered by URL must not take an unreasonable time to download and must not have a low install success rate.
- MSIX package limit is 25 GB. The fourth version field is reserved for the Store (https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements, updated 2026-08-24).

### 2.2 What Tauri supports

- Tauri's official guide covers only EXE (NSIS) and MSI and says Tauri "only generates EXE and MSI installers" (https://v2.tauri.app/distribute/microsoft-store/, last updated 2026-06-15). Its rules are the following. Use a separate config with `webviewInstallMode` set to `offlineInstaller`. Build with `tauri build --no-bundle`, then `tauri bundle --config`. Silent flag `/S` for NSIS or `/quiet` for MSI. Set `bundle.publisher` so it differs from the product name. The installer must be offline and code-signed. The guide says the installer handles auto-updates through the updater plugin.
- Tauri has no official MSIX output. Microsoft's own "Using winapp CLI with Tauri" guide (https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri, 2026-10-07) shows `winapp init` to create `Package.appxmanifest` and `winapp pack` to build an MSIX. It says the Store signs MSIX for you and one MSIX may be needed per architecture. The CLI is in public preview per search results. The guide stages only the `.exe`, so the sidecar, models and resources would need to be staged by hand (**inference**).

### 2.3 Partner Center account

Source is https://learn.microsoft.com/en-us/windows/apps/publish/partner-center/open-a-developer-account (2026-10-29).

- No registration fee for either type when signing up through https://storedeveloper.microsoft.com.
- Individual accounts need government ID plus selfie verification. They are meant for distribution "not in relation to their business, trade, or profession".
- Company accounts cover "independent developers and freelancers" acting in a trade, and businesses. Verification is by D-U-N-S (faster) or official business documents (manual review 2 to 5 business days). A work email on the company domain is required.
- An individual account cannot be converted to a company account.
- Policy 10.14 says the same thing. Company accounts must be used by any person acting in relation to their trade or profession (Store Policies 7.20).

### 2.4 Pricing, trials, commission

- Policy 10.8.1. Non-game PC apps may use either a secure third-party purchase API or the Microsoft Store in-product purchase API for digital items. Only games and Xbox products must use Microsoft's.
- Policy 10.8.2. With third-party commerce the app must name the provider, authenticate the user, confirm the transaction, meet PCI DSS, and declare it in Partner Center. After install, users may be sent to a browser to pay.
- Policy 10.8.4. Trial scope and terms must be clear, and users must be warned in advance if their own content is restricted during or after a trial.
- Store fee (App Developer Agreement v8.11, published 2026-03-17, effective 2026-04-17, https://go.microsoft.com/fwlink/?linkid=528905). 15% of net receipts for non-game apps that use Microsoft commerce. 12% for games. Purchases on a third-party commerce engine "are not subject to the Store Fee".
- MSI and EXE pricing. Partner Center lists Free, Freemium, Subscription and Paid, plus a free-trial setting (https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/price-and-availability, 2026-08-03). The pricing page says Paid means a one-time payment "collected through in-app purchase" (https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/set-app-pricing, 2022-10-30, updated 2024-07-23). A December 2025 Microsoft Q&A post reports that an EXE or MSI app could not upgrade a trial through Store payments (https://learn.microsoft.com/en-us/answers/questions/5656944/, unanswered, unofficial). Treat Store-collected payment for an MSI or EXE listing as **unconfirmed**.
- Native trials use `StoreContext` and `StoreAppLicense.IsTrial` in `Windows.Services.Store`. Desktop Bridge (packaged) apps need extra `StoreContext` setup (https://learn.microsoft.com/en-us/windows/uwp/monetize/implement-a-trial-version-of-your-app, 2017-08-25, updated 2025-06-10). **Inference.** This needs package identity, which means MSIX.

### 2.5 Code, updaters, large downloads

- 10.2.2. No dynamic inclusion of code that changes described function. The example given is downloading and running a remote script.
- 10.2.3. No offers to install unrelated secondary software.
- 10.2.4. A dependency on non-integrated software must be disclosed at the start of the description.
- 10.2.5. Store-only install and update applies to games and Xbox products. It does not apply to 10.2.9 installer submissions. No Store policy I found forbids a self-updater in a non-game MSI or EXE app. Tauri's own Store guide expects the updater plugin.
- 10.2.7. The product must uninstall cleanly.
- **Inference.** Downloading ONNX models after install is data, not code, so 10.2.2 likely does not apply if the features are described. The installer itself must stay offline (10.2.9), so models needed at first run belong inside the installer.

### 2.6 WebView2, privacy, age rating, name

- WebView2. The Evergreen runtime ships with Windows 11. Most Windows 10 devices already have it. Apps should check and install it if missing. MSIX via App Installer can declare it as an external dependency (https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution, 2024-06-27, updated 2026-10-02). For the Store installer Tauri requires `offlineInstaller` mode (section 2.2).
- Privacy. Policy 10.5.1 says Win32 and Desktop Bridge products "must always have privacy policies", with the URL entered in Partner Center. 10.5.2 requires opt-in consent before sending personal data to a third party. 10.5.7 requires a setting to turn location use on and off for apps that use device location.
- Age rating. An IARC questionnaire in Partner Center gives regional ratings. An existing IARC ID can be reused (https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/age-ratings, 2026-08-21). The MSI checklist also requires it (https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/create-app-submission, updated 2026-08-24).
- Name. Every Store name must be unique. A reservation lasts three months. Microsoft advises against trademarked names and gives a contact route for rights holders (https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/reserve-your-apps-name, updated 2026-08-24).

---

## 3. Both stores

### 3.1 One binary for both stores and direct download

- **macOS.** One binary cannot serve the Mac App Store and direct download. The store build needs sandbox entitlements, an embedded provisioning profile, Apple Distribution signing and a `.pkg`. It must not contain license keys (2.4.5(vi), 3.1.1) or its own updater (2.4.5(vii)). The direct build uses Developer ID signing plus notarisation, and can keep the updater and licence keys. Tauri's merged-config approach (section 1.1) fits two build profiles. Cargo features would gate the updater, licence check and export hook (**inference**).
- **Windows, MSI or EXE path.** The same signed installer can serve the Store and direct download. Policy allows third-party commerce (10.8.1) and does not ban a self-updater for non-game apps. Licence keys and own payment can stay.
- **Windows, MSIX path.** The Store signs, hosts and updates. A separate MSIX build is needed. Whether a self-updater must be removed under MSIX is unconfirmed. The Store handles updates, so a self-updater is redundant at best.

### 3.2 Native paid and trial models

| | Mac App Store | Microsoft Store |
|---|---|---|
| Paid upfront | Yes, priced in App Store Connect | Yes for MSIX. For MSI or EXE, Store billing is unconfirmed |
| Native trial | $0 non-consumable "XX-day Trial" IAP plus a paid unlock IAP | Partner Center trial plus `StoreContext` licence API (packaged apps) |
| Own licence keys | Forbidden | Allowed (third-party commerce, 0% fee) |

### 3.3 Name and trademark process for "Emulsion"

- Apple. App names are unique per localization. A rights holder can file a name claim at https://www.apple.com/legal/internet-services/itunes/appnamenotices/ (https://developer.apple.com/help/app-store-connect/create-an-app-record/add-a-new-app/, no date shown). Creating the app record is how the name is held.
- Microsoft. Use Check availability in Partner Center and reserve for three months (section 2.6).
- Trademark registers to search. The USPTO Trademark Search system (tmsearch.uspto.gov), WIPO Global Brand Database, and EUIPO TMview. Then check national registers for each target market (USPTO guidance at https://www.uspto.gov/trademarks/search/comprehensive-clearance-search-similar-trademarks, via search summary).
- A prior Mac photo cataloguing and editing app named "Emulsion" by The Escapers existed. Press from 2015 says it was sold on the Mac App Store and directly for $49, with face grouping. Softpedia lists version 1.4.1 from December 2016 (https://petapixel.com/2015/06/22/hands-on-emulsion-for-mac-is-a-simpler-alternative-to-adobe-lightroom, https://mac.softpedia.com/get/Graphics/Emulsion-The-Escapers.shtml). These are secondary sources. I found no current Mac App Store listing. This is a fact to check, not a legal conclusion.

---

## 4. Comparison table

| Topic | Mac App Store | Microsoft Store |
|---|---|---|
| Account cost | 99 USD per year | Free (new flow) |
| Entity | Individual or legal-entity organisation with D-U-N-S | Individual only if not in trade. Selling commercially needs a Company account (D-U-N-S or documents) |
| Verification time | D-U-N-S up to 5 + 2 business days. Enrolment time not published | Seconds when automatic. Manual review 2 to 5 business days |
| Package | Signed `.pkg` with sandboxed `.app`, uploaded with `altool` | MSIX (no official Tauri path) or signed MSI or EXE by URL (Tauri official) |
| Signing | Apple Distribution plus Mac Installer Distribution | MSIX re-signed by Microsoft. MSI or EXE needs your own trusted-root cert |
| Sandbox | Mandatory | None for MSI or EXE. MSIX has its own model (not researched) |
| Sidecar | Allowed with only `app-sandbox` + `inherit` | No rule found against it |
| External programs (export hook) | Conflicts with 2.5.2 and sandbox (inference) | Not barred if described (10.2.2, 10.2.4) |
| Self-updater | Forbidden (2.4.5(vii)) | Allowed for non-game MSI or EXE. Store updates MSIX |
| Licence keys | Forbidden (2.4.5(vi), 3.1.1) | Allowed with third-party commerce |
| Commission | 30%, or 15% under Small Business Program | 15% with Microsoft commerce. 0% with own commerce |
| Trial | $0 trial IAP | Partner Center trial with licence API |
| Size limit | 200 GB uncompressed | 25 GB per MSIX |
| Privacy policy | Required (5.1.1) | Required for all Win32 (10.5.1) |
| Age rating | Apple questionnaire (new system 2026) | IARC questionnaire |
| Beta testing | TestFlight for Mac | Not researched |

---

## 5. Impact on the project

1. **Build two macOS products.** A Mac App Store build and a Developer ID build. Add a store config (`tauri.appstore.conf.json`) with entitlements, the provisioning profile and the category. Gate the updater, the licence-key check and the export hook out of the store build at compile time.
2. **Rework catalog file access for the sandbox.** Every folder the catalog references must come from an open panel and be persisted as a security-scoped bookmark. Today's catalog likely stores plain paths. The catalog schema needs a bookmark column, and every file read needs a start and stop access wrapper. This is the largest engineering item.
3. **Re-sign the sidecar with exactly `app-sandbox` and `inherit`.** Test that it works under the sandbox. It can only reach files the main app can reach. It cannot be debugged in its shipped form.
4. **Remove or disable the export-plugin hook on the Mac App Store build.** Decide whether the store version ships without it or replaces it with built-in exporters.
5. **Bundle the ONNX models inside the app on macOS.** Do not download them after install. On Windows, put them in the offline installer.
6. **Implement StoreKit 2 purchases on macOS.** Decide between paid upfront and free with a trial IAP plus an unlock IAP. Pick a third-party Tauri IAP plugin or write a thin Swift bridge. Add a restore-purchases control.
7. **Decide the Windows path.** MSI or EXE by URL is the documented Tauri path. It keeps one installer for all channels, keeps own licence keys and own payment at 0% Store fee, and needs a paid trusted-root code-signing certificate plus hosting. MSIX gives free signing, Store updates and Store commerce, but Tauri has no official MSIX output and the winapp CLI is in preview.
8. **Register entities.** Apple organisation enrolment needs a legal entity and D-U-N-S. Microsoft needs a Company account for commercial sales. Start D-U-N-S early because it gates both.
9. **Write a privacy policy** covering map-tile and place-search requests and the local face data. Link it in both stores and in the app. Add a setting to switch off location-related online features (Microsoft 10.5.7).
10. **Complete compliance items.** Export-compliance key in `Info.plist`, Apple age-rating questionnaire, IARC questionnaire, Paid Apps Agreement with tax and banking, Small Business Program enrolment.
11. **Clear the name.** Search USPTO, WIPO and TMview for "Emulsion" in software classes. Investigate the 2015 Escapers app. Reserve the name in Partner Center and create the App Store Connect record early.

---

## 6. Uncertain or unsourced

- Exact sandbox file-access entitlement keys (`files.user-selected.read-write`, `files.bookmarks.app-scope`, `assets.pictures.read-write`) and the security-scoped bookmark API. Apple's page body did not load. Based on my prior knowledge.
- `com.apple.security.network.client` for map tiles and place search. Not confirmed from an official page this session.
- Whether a sandboxed app's child process launched from a user-chosen external path can run at all, and whether review would reject the export hook. Inferred from 2.5.2 and the inherit rules.
- Whether Apple-hosted asset packs (Background Assets) support macOS, and from which version.
- Whether Transporter is still a supported upload route. Only `altool` is documented by Tauri.
- Minimum macOS SDK or Xcode version for macOS uploads in 2026. Apple's pages list none for macOS.
- Apple's total enrolment processing time. Only D-U-N-S timing is official.
- Standard 30% Apple commission and the tax and banking steps came from Apple pages read through search summaries, not full fetches.
- Whether a Microsoft Store MSI or EXE listing can be sold as Paid with Microsoft collecting the money. The official page is ambiguous and a community report says no.
- Whether `StoreContext` licence and trial APIs require package identity (MSIX). Inferred from the Desktop Bridge note.
- Whether MSIX Store builds may keep a self-updater. No policy found either way.
- How MSIX handles a Tauri sidecar, bundled resources and WebView2 in practice. Not researched in depth.
- winapp CLI preview status comes from a search summary, not the fetched guide.
- Microsoft's handling of apps that download large model files after install. No explicit policy found. My reading of 10.2.2 is inference.
- Status of the earlier "Emulsion" app by The Escapers and any trademark it holds. Secondary sources only.
- Microsoft Store beta or flight testing for MSI or EXE apps. Not researched.
