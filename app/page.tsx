import { Faq } from "@/components/faq";
import { Features } from "@/components/features";
import { Hero } from "@/components/hero";
import { History } from "@/components/history";
import { Steps } from "@/components/steps";

const Home = () => (
  <main>
    <Hero />
    <Steps />
    <History />
    <Features />
    <Faq />
  </main>
);

export default Home;
