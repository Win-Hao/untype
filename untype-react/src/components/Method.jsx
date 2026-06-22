import { useEffect, useRef } from 'react'
import { asset } from '../lib/asset.js'

const STEPS = [
  { num: '01', title: '唤起', desc: '按下 ⌥ Space，听写胶囊出现，准备就绪。', chips: ['⌥ Space', '全局唤起', '任意应用'] },
  { num: '02', title: '开口', desc: '正常说话，波形随声起伏，确认它正在听。', chips: ['实时波形', '听写中', '多语言'] },
  { num: '03', title: '成文', desc: '实时转写，自动加标点、断句、分段。', chips: ['自动标点', '智能断句', '分段落'] },
  { num: '04', title: '落定', desc: '松开快捷键，文字落进当前光标处。', chips: ['光标处插入', '任意输入框', '无需粘贴'] },
]

// 浮标在图上的散落位置（中心点 %）
const CHIP_POS = [
  { top: '34%', left: '15%' },
  { top: '64%', left: '43%' },
  { top: '40%', left: '73%' },
]

const NAV_OFFSET = 96 // 与 .step-window / .step-nav 的 sticky top 一致

function prefersReduce() {
  return window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

/* V · 使用流程（四步）—— 左侧可点击 step 列表 + 右侧容器窗口堆叠特效。
   卡片排版对齐 open-design.ai：顶部 STEP 白条 + 下方大图铺满 + 图上浮动产品标签。
   窗口 sticky 钉住、overflow 裁切藏住未轮到的卡；滚动进度用 translateY 把下一张卡推上来覆盖。 */
export default function Method() {
  const stackRef = useRef(null)
  const headRef = useRef(null)

  useEffect(() => {
    const stack = stackRef.current
    if (!stack) return
    const slots = Array.prototype.slice.call(stack.querySelectorAll('.step-slot'))
    const win = stack.querySelector('.step-window')
    const navItems = Array.prototype.slice.call(document.querySelectorAll('#method .step-nav-item'))
    const N = slots.length
    if (!win || !N) return

    navItems.forEach((n, k) => n.classList.toggle('active', k === 0))

    if (prefersReduce()) { stack.classList.add('no-stack'); return }

    // 顶部标题区（.method-headwrap）钉在 top:96 处；把它的实测高度写进 --headH，
    // 让左侧 step 列表与右侧卡片窗口的 sticky 锚点（--stickTop）落到标题下方、不重叠。
    const head = headRef.current
    const body = stack.closest('.method-body')
    const nav = body && body.querySelector('.step-nav')
    const measureHead = () => {
      if (!head || !body || window.innerWidth <= 880) return
      body.style.setProperty('--headH', head.offsetHeight + 'px')
    }
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(measureHead)

    let raf = null
    const update = () => {
      raf = null
      if (window.innerWidth <= 880) { if (head) head.style.transform = ''; if (nav) nav.style.transform = ''; return }
      const winH = win.offsetHeight
      const total = stack.offsetHeight - winH
      if (total <= 0) return
      const stickyTop = parseFloat(getComputedStyle(win).top) || NAV_OFFSET
      const top = stack.getBoundingClientRect().top
      const scrolled = Math.min(Math.max(stickyTop - top, 0), total)
      const p = (scrolled / total) * (N - 1)
      slots.forEach((slot, i) => {
        const ty = Math.min(Math.max(i - p, 0), 1) * 100
        slot.style.transform = 'translateY(' + ty + '%)'
        const covered = Math.min(Math.max(p - i, 0), 1)
        slot.style.setProperty('--cover', String(covered))
      })
      const active = Math.min(Math.max(Math.round(p), 0), N - 1)
      navItems.forEach((n, k) => n.classList.toggle('active', k === active))
      // step-stack 滚完后，卡片（step-window）会解除钉住继续上移；而标题（headwrap）与左侧步骤列表
      // （step-nav）钉住范围更长、否则会单独黏在原地错位。让两者同步上移相同距离，三者整体一起离开。
      const extra = Math.max(0, (stickyTop - top) - total)
      const ty = extra > 0 ? 'translateY(' + (-extra) + 'px)' : ''
      if (head) head.style.transform = ty
      if (nav) nav.style.transform = ty
    }
    const onScroll = () => { if (!raf) raf = requestAnimationFrame(update) }
    window.addEventListener('scroll', onScroll, { passive: true })
    const onResize = () => { measureHead(); onScroll() }
    window.addEventListener('resize', onResize)
    measureHead()
    update()
    return () => {
      window.removeEventListener('scroll', onScroll)
      window.removeEventListener('resize', onResize)
      if (raf) cancelAnimationFrame(raf)
    }
  }, [])

  const goTo = (i) => {
    const stack = stackRef.current
    if (!stack) return
    const reduce = prefersReduce()
    if (window.innerWidth <= 880) {
      const slot = stack.querySelectorAll('.step-slot')[i]
      if (slot) {
        const y = window.scrollY + slot.getBoundingClientRect().top - NAV_OFFSET - 12
        window.scrollTo({ top: Math.max(0, y), behavior: reduce ? 'auto' : 'smooth' })
      }
      return
    }
    const win = stack.querySelector('.step-window')
    if (!win) return
    const stickyTop = parseFloat(getComputedStyle(win).top) || NAV_OFFSET
    const total = stack.offsetHeight - win.offsetHeight
    const stackAbsTop = window.scrollY + stack.getBoundingClientRect().top
    const frac = STEPS.length > 1 ? i / (STEPS.length - 1) : 0
    const target = stackAbsTop - stickyTop + frac * total
    window.scrollTo({ top: Math.max(0, Math.round(target)), behavior: reduce ? 'auto' : 'smooth' })
  }

  return (
    <section id="method">
      <div className="container">
        <div className="method-headwrap" ref={headRef}>
        <div className="sec-rule">
          <span className="roman">V.</span>
          <span className="meta-grp"><span>使用流程</span><span className="dot-mark">·</span><span>四步，从开口到落定</span></span>
          <span>005 / 008</span>
        </div>
        <div className="method-head">
          <div className="method-head-main" data-reveal="">
            <h2 className="display">四步，<br />从开口到<em>落定</em><span className="dot">.</span></h2>
            <div className="method-breadcrumb">
              {STEPS.map((s, i) => (
                <span className="bc-item" key={s.num}>
                  {i > 0 && <span className="bc-sep">→</span>}
                  <span className="bc-no">{s.num}</span>{s.title}
                </span>
              ))}
            </div>
          </div>
          <div className="right" data-reveal="">
            <span className="plus">+</span>
            <p>整条链路在本地优先完成，文字落回你正在用的窗口，无需复制粘贴。</p>
          </div>
        </div>
        </div>

        <div className="method-body">
          <nav className="step-nav" aria-label="使用流程步骤">
            {STEPS.map((s, i) => (
              <button type="button" className="step-nav-item" key={s.num} onClick={() => goTo(i)}>
                <span className="step-nav-no">{s.num}</span>
                <span className="step-nav-title">{s.title}</span>
              </button>
            ))}
          </nav>

          <div className="step-stack" ref={stackRef} style={{ '--numcards': STEPS.length }}>
            <div className="step-window">
              {STEPS.map((s, i) => (
                <div
                  className="step-slot"
                  key={s.num}
                  style={{ '--i': i, transform: i === 0 ? 'translateY(0%)' : 'translateY(100%)' }}
                >
                  <div className="step-card">
                    <div className="step-bar">
                      <span className="step-pill">STEP&nbsp;<b>{s.num}</b></span>
                      <span className="step-desc"><b>{s.title}</b>{s.desc}</span>
                    </div>
                    <div className="step-figure">
                      <img src={asset(`assets/step-${i + 1}.jpg`)} alt={`步骤 ${s.num}：${s.title}`} loading="lazy" />
                      {s.chips.map((label, k) => (
                        <span className="step-chip" key={label} style={{ top: CHIP_POS[k].top, left: CHIP_POS[k].left }}>{label}</span>
                      ))}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="method-foot">
          <span className="left"><span className="ring" />本地优先 · 隐私至上</span>
          <span className="right">平均 <b>四步</b> · 无需鼠标</span>
        </div>
      </div>
    </section>
  )
}
