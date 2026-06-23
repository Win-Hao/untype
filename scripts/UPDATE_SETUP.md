# 自动更新 — 配置与发版指南

Untype 用官方 `tauri-plugin-updater` 做**应用内自动更新**:旧版本启动后静默检查 → 有新版弹窗 → 用户点「立即更新」→ 应用内下载 + 安装 + 自动重启。

macOS 上要让这套跑通,必须满足三件事:**① 代码签名(Developer ID) ② Apple 公证 ③ 更新签名(minisign)**。缺公证的话,自动下载的新包会被 Gatekeeper 拦,更新即损坏。

---

## 一、一次性配置

所有凭据写进 `scripts/signing.local.env`(已 gitignore,**不会提交**)。建好后内容形如:

```bash
# ① 代码签名(你已有 Developer ID 证书)
export APPLE_SIGNING_IDENTITY="Developer ID Application: 你的名字 (TEAMID)"

# ② 公证 —— App Store Connect API Key(见下方步骤)
export APPLE_API_ISSUER="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"   # Issuer ID
export APPLE_API_KEY="XXXXXXXXXX"                                 # Key ID
export APPLE_API_KEY_PATH="$HOME/.appstoreconnect/AuthKey_XXXXXXXXXX.p8"

# ③ 更新签名(minisign)——可省略:build-mac.sh 默认读 ~/.tauri/untype-updater.key
# export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/untype-updater.key)"
# export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
```

### 查 `APPLE_SIGNING_IDENTITY` 准确名字
```bash
security find-identity -v -p codesigning | grep "Developer ID Application"
```
把引号里那串完整名字(含 Team ID)填进去。

### ② 生成 App Store Connect API Key(公证用)
1. 打开 https://appstoreconnect.apple.com/ → 右上「用户和访问 Users and Access」。
2. 顶部切到「集成 Integrations」标签 →左侧「App Store Connect API」→「团队密钥 Team Keys」。
3. 点「生成 API 密钥 (+)」,名字随便(如 `untype-notarize`),**访问权限选「Developer」**(公证够用),生成。
4. **下载那个 `.p8` 文件**(只能下一次!),记下这一行的 **Key ID**(那串 10 位)。
   Key id:KSUVYWZDHP
5. 同页顶部能看到 **Issuer ID**(一长串 UUID),也记下。
   Issuer id:b87832a1-3f43-4477-b9d4-655b8ab94eb1
6. 把 .p8 放到安全位置并填进 env:
   ```bash
   mkdir -p ~/.appstoreconnect
   mv ~/Downloads/AuthKey_XXXXXXXXXX.p8 ~/.appstoreconnect/
   ```
   然后把 `APPLE_API_KEY`(=Key ID)、`APPLE_API_ISSUER`(=Issuer ID)、`APPLE_API_KEY_PATH` 填进 `signing.local.env`。

> 也可以用 Apple ID + 专用密码代替(设 `APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID`),但密码会过期,不如 API Key 省心。

### ③ 更新签名密钥(已生成)
- 已为你生成 minisign 密钥对,公钥**已写进** `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。
- 私钥在 `~/.tauri/untype-updater.key`(无密码)。`build-mac.sh` 会自动读取。
- ⚠️ **务必备份这个私钥文件**。一旦丢失,已安装用户将再也无法验证你后续发布的更新 = 断更,且无法找回。建议复制到密码管理器或离线备份。

---

## 二、发一个新版本

```bash
# 1) 三处版本号一起改(保持一致),比如 0.1.0 → 0.1.1
#    - src-tauri/tauri.conf.json   "version"
#    - src-tauri/Cargo.toml        version
#    - package.json                version

# 2) (可选) 写更新说明,会进 release 和弹窗的「更新内容」
echo "• 修复了 xxx\n• 新增 yyy" > RELEASE_NOTES.md

# 3) 先提交并推送代码 —— 务必在发版之前!
#    release-mac.sh 用 gh 在「远端当前 HEAD」上打 tag,先 push 才能让 v0.x.y
#    这个 tag 指向正确的提交;否则 tag 会指向旧代码(二进制虽对、但源码对不上)。
git add -A && git commit -m "release: vX.Y.Z" && git push

# 4) 构建(签名 + 公证 + 产出 updater 产物;公证要几分钟)
bash scripts/build-mac.sh

# 5) 生成 latest.json(更新清单)
bash scripts/gen-latest-json.sh

# 6) 发布到 GitHub Releases(需 gh CLI 已登录)
bash scripts/release-mac.sh

# 7) 更新官网下载直链 —— untype-react/src/lib/links.js 的 DOWNLOAD_DMG 改成新版本号
#    （Hero/导航是直链、含版本号，不会自动更新；CTA 用的 DOWNLOAD_URL=releases/latest 会自动）
#    改完 commit + push 会触发 Pages 自动部署。
```

发布后,装了旧版本的用户下次打开 app 约 3 秒后就会弹出更新提示。

> 注意:发版必须是「正式 release」(非 prerelease/draft),否则 `releases/latest` 端点取不到 latest.json,更新检查会落空。`release-mac.sh` 默认就发正式版。

---

## 三、验证整条链路

1. 当前 `0.1.0` 构建并安装到 /Applications。
2. 版本号 bump 到 `0.1.1`,跑上面 3→5 发布。
3. 打开 `0.1.0` 的 Untype → 启动应弹「发现新版本 0.1.1」→ 点「立即更新」→ 看到下载进度 → 自动重启到 0.1.1。

关于页有「检查更新」可手动触发;开发期(`npm run tauri dev`)有「预览更新弹窗(dev)」可只看弹窗 UI(不下载)。

---

## 四、常见问题

- **公证被拒,报 dylib「not signed / missing secure timestamp」**:开了 hardenedRuntime 后,捆绑的 `libonnxruntime` / `libsherpa-onnx-c-api` 必须带时间戳签名。若 Tauri 默认签名没覆盖到,在 `build-mac.sh` 的 `npm run tauri build` 前补一段对这两个 dylib 单独签:
  ```bash
  codesign --force --timestamp --options runtime \
    --sign "$APPLE_SIGNING_IDENTITY" \
    src-tauri/target/release/libonnxruntime.1.17.1.dylib \
    src-tauri/target/release/libsherpa-onnx-c-api.dylib
  ```
- **更新检查不到**:确认 release 是「正式版」(非 prerelease)、`latest.json` 已作为资源上传、其中 `version` 比当前高、`url` 指向同一 release 的 `Untype.app.tar.gz`。
- **下载完没自动重启**(Tauri v2 macOS 已知 bug):已内置 `force_quit_and_relaunch` 兜底;若仍失败,弹窗会提示「手动退出后重开」。
- **架构**:目前只出 Apple Silicon(aarch64)。Intel Mac 不在 `latest.json` 的 `platforms` 里,不会收到更新。
