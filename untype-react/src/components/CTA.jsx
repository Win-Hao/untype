import { asset } from '../lib/asset.js'
import { DOWNLOAD_URL, PROFILE_URL } from '../lib/links.js'

/* VIII · 下载 CTA */
export default function CTA() {
  return (
    <section className="cta" id="cta">
      <div className="container">
        <div className="sec-rule">
          <span className="roman">VIII.</span>
          <span className="meta-grp"><span>开始使用</span><span className="dot-mark">·</span><span>下载 Untype</span></span>
          <span>008 / 008</span>
        </div>
        <div className="cta-grid">
          <div>
            <span className="label" data-reveal="">开始使用<span className="ix">VIII</span></span>
            <h2 className="display" data-reveal="">别再<em>打字</em>了<span className="dot">.</span></h2>
            <p className="lead" data-reveal="">下载 Untype，按一下快捷键，把第一句话说出来。文字会自己落定。</p>
            <div className="cta-actions" data-reveal="">
              <a className="btn btn-primary" href={DOWNLOAD_URL} target="_blank" rel="noopener noreferrer">下载 macOS 版 <span className="arrow"><svg viewBox="0 0 24 24"><path d="M12 4v12M6 12l6 6 6-6" /></svg></span></a>
              <a className="email-pill" href={PROFILE_URL} target="_blank" rel="noopener noreferrer">github.com/Win-Hao <span className="arrow-circle">↗</span></a>
            </div>
            <p className="cta-note" data-reveal="">开源自构建，未做苹果公证。首次打开若被系统拦下：右键点应用图标 →「打开」，或在终端执行 <code className="code-inline">xattr -dr com.apple.quarantine /Applications/Untype.app</code></p>
            <div className="cta-foot" data-reveal="">
              <span className="stamp">免费试用</span>
              <span>macOS 12+ · Apple Silicon</span>
              <span>本地优先 · 隐私至上</span>
            </div>
          </div>
          <div className="cta-art" data-reveal="right">
            <span className="ribbon">FIN. · MMXXVI</span>
            <span className="index">∞</span>
            <img src={asset('assets/cta.jpg')} alt="开始使用 Untype：拱门与开阔天空" />
          </div>
        </div>
      </div>
    </section>
  )
}
