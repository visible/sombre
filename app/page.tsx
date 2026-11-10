import { Faq } from "@/components/faq";
import { Features } from "@/components/features";
import { Footer } from "@/components/footer";
import { Header } from "@/components/header";
import { Hero } from "@/components/hero";
import { History } from "@/components/history";
import { Steps } from "@/components/steps";

const Home = () => (
  <div className="border-line mx-auto max-w-[1440px] xl:border-x">
    <Header />
    <main>
      <Hero />
      <Steps />
      <History />
      <Features />
      <Faq />
    </main>
    <Footer />
  </div>
);

export default Home;
