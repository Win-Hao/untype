import { useReveals } from './hooks/useReveals.js'
import SideRails from './components/SideRails.jsx'
import Topbar from './components/Topbar.jsx'
import Nav from './components/Nav.jsx'
import Hero from './components/Hero.jsx'
import Wire from './components/Wire.jsx'
import About from './components/About.jsx'
import Capabilities from './components/Capabilities.jsx'
import Labs from './components/Labs.jsx'
import Method from './components/Method.jsx'
import Work from './components/Work.jsx'
import Testimonial from './components/Testimonial.jsx'
import CTA from './components/CTA.jsx'
import Footer from './components/Footer.jsx'

export default function App() {
  // 挂载后接管所有 [data-reveal] 元素的滚动揭示（GSAP，含降级）
  useReveals()

  return (
    <div className="shell">
      <SideRails />
      <Topbar />
      <Nav />
      <Hero />
      <Wire />
      <About />
      <Capabilities />
      <Labs />
      <Method />
      <Work />
      <Testimonial />
      <CTA />
      <Footer />
    </div>
  )
}
