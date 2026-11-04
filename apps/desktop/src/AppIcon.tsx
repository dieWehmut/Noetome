import { HouseIcon } from "@phosphor-icons/react/dist/csr/House";
import { ClockIcon } from "@phosphor-icons/react/dist/csr/Clock";
import { InfoIcon } from "@phosphor-icons/react/dist/csr/Info";
import { ArrowRightIcon } from "@phosphor-icons/react/dist/csr/ArrowRight";
import { BookOpenIcon } from "@phosphor-icons/react/dist/csr/BookOpen";
import { TerminalWindowIcon } from "@phosphor-icons/react/dist/csr/TerminalWindow";
import { GearSixIcon } from "@phosphor-icons/react/dist/csr/GearSix";
import { SunIcon } from "@phosphor-icons/react/dist/csr/Sun";
import { MoonIcon } from "@phosphor-icons/react/dist/csr/Moon";
import { FolderOpenIcon } from "@phosphor-icons/react/dist/csr/FolderOpen";
import { CaretDownIcon } from "@phosphor-icons/react/dist/csr/CaretDown";
import { MagnifyingGlassIcon } from "@phosphor-icons/react/dist/csr/MagnifyingGlass";
import { NotePencilIcon } from "@phosphor-icons/react/dist/csr/NotePencil";
import { DotsThreeIcon } from "@phosphor-icons/react/dist/csr/DotsThree";
import { AppWindowIcon } from "@phosphor-icons/react/dist/csr/AppWindow";

const icons = {
  home: HouseIcon, pulse: ClockIcon, info: InfoIcon, arrow: ArrowRightIcon,
  book: BookOpenIcon, terminal: TerminalWindowIcon, settings: GearSixIcon,
  sun: SunIcon, moon: MoonIcon, folder: FolderOpenIcon, caret: CaretDownIcon,
  search: MagnifyingGlassIcon, compose: NotePencilIcon, more: DotsThreeIcon,
  window: AppWindowIcon,
};

export default function AppIcon({ name, filled = false }: { name: keyof typeof icons; filled?: boolean }) {
  const Icon = icons[name];
  return <Icon aria-hidden="true" size={22} weight={filled ? "fill" : "regular"} />;
}
