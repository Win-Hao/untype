/* 顶部期刊元信息条 */
export default function Topbar() {
  return (
    <div className="topbar">
      <div className="container">
        <div className="topbar-inner">
          <span><b>Vol. 01</b> · Issue Nº 01 · 听写工具</span>
          <span className="mid">
            <span>Filed under · 语音输入 / 听写</span>
            <span>本地优先 · 隐私至上</span>
          </span>
          <span className="right">
            <span><span className="pulse" />实时转写 · 在线</span>
            <span>v1.0 · zh-CN</span>
          </span>
        </div>
      </div>
    </div>
  )
}
