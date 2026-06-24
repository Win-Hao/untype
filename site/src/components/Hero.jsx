import { asset } from '../lib/asset.js'
import { DOWNLOAD_DMG } from '../lib/links.js'
import StarButton from './StarButton.jsx'

/* I · 主视觉 */
export default function Hero() {
  return (
    <section className="hero" id="top" style={{ paddingTop: '40px' }}>
      <div className="container hero-grid">
        <div className="hero-copy">
          <span className="label" data-reveal="">听写工具 · macOS<span className="ix">I</span></span>
          <h1 className="display" data-reveal="">开口说话，<em>文字</em><br />自己落定<span className="dot">.</span></h1>
          <p className="lead" data-reveal="">Untype 是一款极简语音转文字工具。按下快捷键，开口说话，标点与分段自动整理，文字直接落进你正在用的任何输入框 —— 不打字，也能成文。</p>
          <div className="hero-actions" data-reveal="">
            <a className="btn btn-primary" href={DOWNLOAD_DMG}>下载 macOS 版 <span className="arrow"><svg viewBox="0 0 24 24"><path d="M5 12h14M13 6l6 6-6 6" /></svg></span></a>
            <StarButton />
          </div>
          <div className="hero-stats" data-reveal="">
            <span className="stat"><span className="ring solid">1</span><span className="stat-label">一个快捷键<b>全局唤起</b></span></span>
            <span className="stat"><span className="ring">∞</span><span className="stat-label">任意输入框<b>直接插入</b></span></span>
            <span className="stat"><span className="ring coral">Aa</span><span className="stat-label">标点与分段<b>自动整理</b></span></span>
          </div>
          <div className="hero-foot" data-reveal="">
            <span className="meta">FILED UNDER<br /><b style={{ color: 'var(--ink)' }}>语音 → 文字 · 实时</b></span>
            <span className="coord">VOL. 01 · MMXXVI · ⌥ Space</span>
          </div>
        </div>
        <div className="hero-art">
          <div className="corner tl" /><div className="corner tr" />
          <div className="corner bl" /><div className="corner br" />
          <span className="annot annot-tl">PLATE Nº 01</span>
          <span className="annot annot-tr coord">声波 → 基线 → 光标</span>
          <span className="annot annot-bl coord">⌥ + Space</span>
          <img src={asset('assets/hero.jpg')} alt="Untype 主视觉：声波收敛为一行文字" loading="eager" />
          <div className="index">
            <span className="on"><span className="n">01</span>唤起</span>
            <span><span className="n">02</span>说话</span>
            <span><span className="n">03</span>成文</span>
            <span><span className="n">04</span>落定</span>
          </div>
        </div>
      </div>
    </section>
  )
}
