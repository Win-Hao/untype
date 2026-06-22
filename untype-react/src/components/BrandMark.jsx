import { BRAND_MARK } from '../lib/plates.js'

/* 圆形品牌标记：波形 → 光标的迷你图标 */
export default function BrandMark() {
  return <span className="brand-mark" dangerouslySetInnerHTML={{ __html: BRAND_MARK }} />
}
