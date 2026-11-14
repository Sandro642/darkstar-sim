//! Version développeur (instruction DEV) : poste de travail sur fond blanc, barre de menus en haut
//! (FENETRES, CONSOLE, DISPOSITION, SIMULATEUR) et fenêtres déplaçables / redimensionnables :
//! ECRAN (grand afficheur à pages), MESSAGES / GCS, TRAIN, ND, MOTEURS, ELEC, PANNEAU DROIT
//! (moteurs et APU), PANNES. L'ECRAN a 7 pages (1 HORIZON, 2 FCS, 3 THERMIQUE, 4 RECON,
//! 5 SYSTEMES avec ARMEMENT et DONNEES MOTEUR, 6 INCENDIE, 7 MCDU) et une page CONFIG : canevas
//! où l'on glisse-dépose les éléments de la palette pour composer la page 1. La disposition est
//! enregistrée sur le disque.

use crate::ui::*;
use macroquad::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinKind {
    Ecran,
    Messages,
    Train,
    Nd,
    Moteurs,
    Elec,
    PanneauDroit,
    Pannes,
}

pub const WINS: [(WinKind, &str); 8] = [
    (WinKind::Ecran, "ECRAN"),
    (WinKind::Messages, "MESSAGES / GCS"),
    (WinKind::Train, "TRAIN"),
    (WinKind::Nd, "ND"),
    (WinKind::Moteurs, "MOTEURS"),
    (WinKind::Elec, "ELEC"),
    (WinKind::PanneauDroit, "PANNEAU DROIT"),
    (WinKind::Pannes, "PANNES"),
];

fn win_name(k: WinKind) -> &'static str {
    WINS.iter().find(|w| w.0 == k).map(|w| w.1).unwrap_or("")
}

/// Éléments de la page 1 (composée dans CONFIG).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elem {
    Horizon,
    DonneesMoteur,
    Systemes,
    Fcs,
    Alarmes,
    Nd,
    Thermique,
    Elec,
    Train,
    Messages,
    Mcdu,
    Incendie,
    Limites,
}

pub const ELEMS: [(Elem, &str); 13] = [
    (Elem::Horizon, "HORIZON"),
    (Elem::DonneesMoteur, "DONNEES MOTEUR"),
    (Elem::Systemes, "SYSTEMES"),
    (Elem::Fcs, "FCS"),
    (Elem::Alarmes, "ALARMES"),
    (Elem::Nd, "ND"),
    (Elem::Thermique, "THERMIQUE"),
    (Elem::Elec, "ELEC"),
    (Elem::Train, "TRAIN"),
    (Elem::Messages, "MESSAGES"),
    (Elem::Mcdu, "MCDU"),
    (Elem::Incendie, "INCENDIE"),
    (Elem::Limites, "LIMITES"),
];

pub fn elem_name(e: Elem) -> &'static str {
    ELEMS.iter().find(|x| x.0 == e).map(|x| x.1).unwrap_or("")
}

/// Pages de l'ECRAN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Horizon,
    Fcs,
    Thermique,
    Recon,
    Systemes,
    Incendie,
    Mcdu,
    Config,
    Armement,
    DonneesMoteur,
}

const PAGES: [(Page, &str); 8] = [
    (Page::Horizon, "1 HORIZON"),
    (Page::Fcs, "2 FCS"),
    (Page::Thermique, "3 THERMIQUE"),
    (Page::Recon, "4 RECON"),
    (Page::Systemes, "5 SYSTEMES"),
    (Page::Incendie, "6 INCENDIE"),
    (Page::Mcdu, "7 MCDU"),
    (Page::Config, "CONFIG"),
];

#[derive(Clone, Copy, Debug)]
pub struct Win {
    pub kind: WinKind,
    /// Position et taille en fractions de l'espace de travail
    pub r: [f32; 4],
    pub open: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Widget {
    pub elem: Elem,
    /// Position et taille en fractions de la page
    pub r: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    Move { win: usize, off: Vec2 },
    Resize { win: usize },
    WidMove { i: usize, off: Vec2 },
    WidResize { i: usize },
    Palette { elem: Elem },
}

/// Actions de la barre de menus pour le simulateur.
#[derive(Default)]
pub struct BarOut {
    pub quit: bool,
    pub console_open: Option<bool>,
}

pub struct DevUi {
    pub wins: Vec<Win>,
    pub widgets: Vec<Widget>,
    pub page: Page,
    menu: Option<usize>,
    drag: Option<Drag>,
    /// Zone de travail de la dernière image (sous la barre)
    work: Rect,
    /// Version normale : l'écran principal affiche la page 1 composée
    pub main_screen: bool,
}

pub const BAR_H: f32 = 30.0;

fn path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("Library/Application Support/Darkstar/dev_layout.txt")
}

fn default_wins() -> Vec<Win> {
    let w = |kind, x, y, ww, h, open| Win { kind, r: [x, y, ww, h], open };
    vec![
        w(WinKind::Moteurs, 0.005, 0.01, 0.20, 0.55, true),
        w(WinKind::Ecran, 0.21, 0.01, 0.52, 0.72, true),
        w(WinKind::PanneauDroit, 0.74, 0.01, 0.255, 0.40, true),
        w(WinKind::Pannes, 0.74, 0.42, 0.255, 0.31, true),
        w(WinKind::Elec, 0.005, 0.57, 0.20, 0.42, true),
        w(WinKind::Messages, 0.21, 0.74, 0.40, 0.25, true),
        w(WinKind::Train, 0.615, 0.74, 0.115, 0.25, true),
        w(WinKind::Nd, 0.74, 0.74, 0.255, 0.25, false),
    ]
}

fn default_widgets() -> Vec<Widget> {
    vec![
        Widget { elem: Elem::Horizon, r: [0.0, 0.0, 0.70, 1.0] },
        Widget { elem: Elem::DonneesMoteur, r: [0.70, 0.0, 0.30, 0.5] },
        Widget { elem: Elem::Alarmes, r: [0.70, 0.5, 0.30, 0.5] },
    ]
}

fn kind_of(name: &str) -> Option<WinKind> {
    WINS.iter().find(|w| w.1.replace(' ', "_") == name).map(|w| w.0)
}

fn elem_of(name: &str) -> Option<Elem> {
    ELEMS.iter().find(|e| e.1.replace(' ', "_") == name).map(|e| e.0)
}

impl DevUi {
    pub fn new() -> DevUi {
        let mut d = DevUi { wins: default_wins(), widgets: default_widgets(), page: Page::Horizon, menu: None, drag: None, work: Rect::new(0.0, BAR_H, 1.0, 1.0), main_screen: false };
        d.load();
        d
    }

    fn load(&mut self) {
        let Ok(s) = std::fs::read_to_string(path()) else { return };
        let mut wins = Vec::new();
        let mut wids = Vec::new();
        for l in s.lines() {
            let f: Vec<&str> = l.split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f32>().ok());
            match f.first() {
                Some(&"W") => {
                    if let (Some(k), Some(x), Some(y), Some(w), Some(h)) = (f.get(1).and_then(|n| kind_of(n)), num(2), num(3), num(4), num(5)) {
                        wins.push(Win { kind: k, r: [x, y, w, h], open: f.get(6) == Some(&"1") });
                    }
                }
                Some(&"M") => self.main_screen = f.get(1) == Some(&"1"),
                Some(&"E") => {
                    if let (Some(e), Some(x), Some(y), Some(w), Some(h)) = (f.get(1).and_then(|n| elem_of(n)), num(2), num(3), num(4), num(5)) {
                        wids.push(Widget { elem: e, r: [x, y, w, h] });
                    }
                }
                _ => {}
            }
        }
        // Fenêtres manquantes (nouvelles versions) : position par défaut
        for d in default_wins() {
            if !wins.iter().any(|w| w.kind == d.kind) {
                wins.push(d);
            }
        }
        if !wins.is_empty() {
            self.wins = wins;
        }
        self.widgets = wids;
    }

    pub fn save(&self) {
        let mut s = String::new();
        for w in &self.wins {
            s.push_str(&format!("W {} {:.4} {:.4} {:.4} {:.4} {}\n", win_name(w.kind).replace(' ', "_"), w.r[0], w.r[1], w.r[2], w.r[3], w.open as u8));
        }
        s.push_str(&format!("M {}\n", self.main_screen as u8));
        for e in &self.widgets {
            s.push_str(&format!("E {} {:.4} {:.4} {:.4} {:.4}\n", elem_name(e.elem).replace(' ', "_"), e.r[0], e.r[1], e.r[2], e.r[3]));
        }
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, s);
    }

    fn rect_of(&self, w: &Win) -> Rect {
        let a = self.work;
        Rect::new(a.x + w.r[0] * a.w, a.y + w.r[1] * a.h, w.r[2] * a.w, w.r[3] * a.h)
    }

    /// Début d'image : fond blanc, zone de travail, déplacement / redimensionnement des fenêtres.
    pub fn begin(&mut self, sw: f32, sh: f32) {
        clear_background(Color::new(0.94, 0.95, 0.96, 1.0));
        self.work = Rect::new(4.0, BAR_H + 4.0, sw - 8.0, sh - BAR_H - 8.0);
        let (mx, my) = mouse_position();
        let m = vec2(mx, my);
        if self.menu.is_some() {
            return;
        }
        // Fin de glisser d'une fenêtre (les glisser de la page CONFIG sont traités par la page,
        // qui pose l'élément au relâchement) ; glisser CONFIG orphelin si la page a changé
        if !is_mouse_button_down(MouseButton::Left) {
            match self.drag {
                Some(Drag::Move { .. } | Drag::Resize { .. }) => {
                    self.drag = None;
                    self.save();
                }
                Some(_) if self.page != Page::Config => self.drag = None,
                _ => {}
            }
        }
        let a = self.work;
        match self.drag {
            Some(Drag::Move { win, off }) => {
                let w = &mut self.wins[win];
                w.r[0] = ((m.x - off.x - a.x) / a.w).clamp(-w.r[2] + 0.05, 0.95);
                w.r[1] = ((m.y - off.y - a.y) / a.h).clamp(0.0, 0.95);
            }
            Some(Drag::Resize { win }) => {
                let w = &mut self.wins[win];
                w.r[2] = ((m.x - a.x) / a.w - w.r[0]).max(0.06);
                w.r[3] = ((m.y - a.y) / a.h - w.r[1]).max(0.06);
            }
            _ => {}
        }
        // Nouveau glisser : fenêtre la plus haute sous la souris (barre de titre ou coin)
        if is_mouse_button_pressed(MouseButton::Left) && self.drag.is_none() {
            for i in (0..self.wins.len()).rev() {
                if !self.wins[i].open {
                    continue;
                }
                let r = self.rect_of(&self.wins[i]);
                if !r.contains(m) {
                    continue;
                }
                // Premier plan
                let w = self.wins.remove(i);
                self.wins.push(w);
                let top = self.wins.len() - 1;
                let title = Rect::new(r.x, r.y, r.w - 26.0, 20.0);
                let corner = Rect::new(r.x + r.w - 14.0, r.y + r.h - 14.0, 14.0, 14.0);
                if title.contains(m) {
                    self.drag = Some(Drag::Move { win: top, off: m - vec2(r.x, r.y) });
                } else if corner.contains(m) {
                    self.drag = Some(Drag::Resize { win: top });
                }
                break;
            }
        }
    }

    /// Fenêtres ouvertes, de l'arrière vers l'avant : (indice, type).
    pub fn order(&self) -> Vec<(usize, WinKind)> {
        self.wins.iter().enumerate().filter(|(_, w)| w.open).map(|(i, w)| (i, w.kind)).collect()
    }

    /// Cadre d'une fenêtre : renvoie la zone de contenu (None si fermée par la croix).
    pub fn frame(&mut self, i: usize) -> Option<Rect> {
        let r = self.rect_of(&self.wins[i]);
        draw_rectangle(r.x + 3.0, r.y + 3.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.12));
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.05, 0.06, 0.07, 1.0));
        draw_rectangle(r.x, r.y, r.w, 20.0, Color::new(0.82, 0.84, 0.87, 1.0));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.55, 0.58, 0.62, 1.0));
        txt(win_name(self.wins[i].kind), r.x + 8.0, r.y + 14.0, 11.0, Color::new(0.1, 0.12, 0.15, 1.0));
        let x = Rect::new(r.x + r.w - 22.0, r.y + 2.0, 18.0, 16.0);
        draw_rectangle(x.x, x.y, x.w, x.h, if hovered(x) { Color::new(0.85, 0.3, 0.3, 1.0) } else { Color::new(0.72, 0.74, 0.77, 1.0) });
        txt_c("X", x.x + x.w / 2.0, x.y + 12.0, 11.0, WHITE);
        // Poignée de redimensionnement
        for k in 0..3 {
            let o = 4.0 + k as f32 * 4.0;
            draw_line(r.x + r.w - o, r.y + r.h - 2.0, r.x + r.w - 2.0, r.y + r.h - o, 1.0, Color::new(0.6, 0.62, 0.66, 1.0));
        }
        if self.menu.is_none() && clicked(x) {
            self.wins[i].open = false;
            self.save();
            return None;
        }
        Some(Rect::new(r.x + 2.0, r.y + 22.0, r.w - 4.0, r.h - 24.0))
    }

    /// Bandeau des pages de l'ECRAN (et sous-pages de SYSTEMES) : renvoie la zone de la page.
    pub fn screen_tabs(&mut self, r: Rect) -> Rect {
        let n = PAGES.len() as f32;
        let bw = r.w / n;
        let bh = 22.0;
        for (k, (p, l)) in PAGES.iter().enumerate() {
            let b = Rect::new(r.x + k as f32 * bw + 1.0, r.y, bw - 2.0, bh);
            let on = self.page == *p || (*p == Page::Systemes && matches!(self.page, Page::Armement | Page::DonneesMoteur));
            draw_rectangle(b.x, b.y, b.w, b.h, if on { Color::new(0.1, 0.32, 0.45, 1.0) } else if hovered(b) { Color::new(0.14, 0.16, 0.19, 1.0) } else { Color::new(0.09, 0.1, 0.12, 1.0) });
            txt_c(l, b.x + b.w / 2.0, b.y + 15.0, 10.0, if on { WHITE } else { Color::new(0.7, 0.74, 0.78, 1.0) });
            if self.menu.is_none() && clicked(b) {
                self.page = *p;
            }
        }
        let mut y = r.y + bh + 2.0;
        if matches!(self.page, Page::Systemes | Page::Armement | Page::DonneesMoteur) {
            let subs = [(Page::Systemes, "SYNOPTIQUES"), (Page::Armement, "ARMEMENT"), (Page::DonneesMoteur, "DONNEES MOTEUR")];
            let sw = 130.0f32.min(r.w / 3.0);
            for (k, (p, l)) in subs.iter().enumerate() {
                let b = Rect::new(r.x + 1.0 + k as f32 * (sw + 2.0), y, sw, 18.0);
                let on = self.page == *p;
                draw_rectangle(b.x, b.y, b.w, b.h, if on { Color::new(0.12, 0.38, 0.30, 1.0) } else { Color::new(0.09, 0.1, 0.12, 1.0) });
                txt_c(l, b.x + b.w / 2.0, b.y + 13.0, 9.0, if on { WHITE } else { Color::new(0.7, 0.74, 0.78, 1.0) });
                if self.menu.is_none() && clicked(b) {
                    self.page = *p;
                }
            }
            y += 20.0;
        }
        Rect::new(r.x, y, r.w, r.y + r.h - y)
    }

    /// Page de l'ECRAN par numéro ou nom (instruction DEV PAGE).
    pub fn set_page(&mut self, p: &str) -> bool {
        let pg = match p {
            "1" | "HORIZON" => Page::Horizon,
            "2" | "FCS" => Page::Fcs,
            "3" | "THERMIQUE" => Page::Thermique,
            "4" | "RECON" => Page::Recon,
            "5" | "SYSTEMES" => Page::Systemes,
            "6" | "INCENDIE" => Page::Incendie,
            "7" | "MCDU" => Page::Mcdu,
            "CONFIG" => Page::Config,
            "ARMEMENT" => Page::Armement,
            "MOTEUR" | "MOTEURS" => Page::DonneesMoteur,
            _ => return false,
        };
        self.page = pg;
        true
    }

    /// Rectangles des éléments de la page 1 dans la zone `area`.
    pub fn page1(&self, area: Rect) -> Vec<(Elem, Rect)> {
        self.widgets.iter().map(|w| (w.elem, Rect::new(area.x + w.r[0] * area.w, area.y + w.r[1] * area.h, w.r[2] * area.w, w.r[3] * area.h))).collect()
    }

    /// Page CONFIG : canevas (aperçu de la page 1) et palette d'éléments à glisser-déposer.
    pub fn config(&mut self, area: Rect) {
        let pal_w = (area.w * 0.24).clamp(120.0, 200.0);
        let canvas = Rect::new(area.x + 6.0, area.y + 24.0, area.w - pal_w - 18.0, area.h - 30.0);
        let pal = Rect::new(canvas.x + canvas.w + 6.0, area.y + 24.0, pal_w, area.h - 30.0);
        txt("PAGE 1 : glisser les elements de la palette, deplacer, redimensionner (coin), X pour retirer", area.x + 6.0, area.y + 15.0, 10.0, Color::new(0.7, 0.74, 0.78, 1.0));
        draw_rectangle(canvas.x, canvas.y, canvas.w, canvas.h, Color::new(0.97, 0.97, 0.98, 1.0));
        // Grille
        for k in 1..10 {
            let x = canvas.x + canvas.w * k as f32 / 10.0;
            let y = canvas.y + canvas.h * k as f32 / 10.0;
            draw_line(x, canvas.y, x, canvas.y + canvas.h, 1.0, Color::new(0.88, 0.89, 0.91, 1.0));
            draw_line(canvas.x, y, canvas.x + canvas.w, y, 1.0, Color::new(0.88, 0.89, 0.91, 1.0));
        }
        let (mx, my) = mouse_position();
        let m = vec2(mx, my);
        let to_n = |p: Vec2| vec2((p.x - canvas.x) / canvas.w, (p.y - canvas.y) / canvas.h);
        let snap = |v: f32| (v * 20.0).round() / 20.0;
        // Glisser en cours
        if !is_mouse_button_down(MouseButton::Left) {
            if let Some(Drag::Palette { elem }) = self.drag {
                if canvas.contains(m) {
                    let n = to_n(m);
                    let (w, h) = (0.35f32, 0.4f32);
                    let x = snap((n.x - w / 2.0).clamp(0.0, 1.0 - w));
                    let y = snap((n.y - h / 2.0).clamp(0.0, 1.0 - h));
                    self.widgets.push(Widget { elem, r: [x, y, w, h] });
                }
            }
            if matches!(self.drag, Some(Drag::Palette { .. } | Drag::WidMove { .. } | Drag::WidResize { .. })) {
                self.drag = None;
                self.save();
            }
        }
        match self.drag {
            Some(Drag::WidMove { i, off }) if i < self.widgets.len() => {
                let n = to_n(m - off);
                let w = &mut self.widgets[i];
                w.r[0] = snap(n.x.clamp(0.0, 1.0 - w.r[2]));
                w.r[1] = snap(n.y.clamp(0.0, 1.0 - w.r[3]));
            }
            Some(Drag::WidResize { i }) if i < self.widgets.len() => {
                let n = to_n(m);
                let w = &mut self.widgets[i];
                w.r[2] = snap((n.x - w.r[0]).clamp(0.1, 1.0 - w.r[0]));
                w.r[3] = snap((n.y - w.r[1]).clamp(0.1, 1.0 - w.r[1]));
            }
            _ => {}
        }
        // Éléments placés
        let mut remove = None;
        let mut start = None;
        for (i, w) in self.widgets.iter().enumerate() {
            let r = Rect::new(canvas.x + w.r[0] * canvas.w, canvas.y + w.r[1] * canvas.h, w.r[2] * canvas.w, w.r[3] * canvas.h);
            let hue = (i as f32 * 0.17).fract();
            let fill = Color::new(0.75 + 0.2 * hue, 0.82, 0.95 - 0.2 * hue, 1.0);
            draw_rectangle(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, fill);
            draw_rectangle_lines(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, 1.5, Color::new(0.2, 0.3, 0.45, 1.0));
            txt_c(elem_name(w.elem), r.x + r.w / 2.0, r.y + r.h / 2.0 + 4.0, 12.0, Color::new(0.08, 0.12, 0.2, 1.0));
            let x = Rect::new(r.x + r.w - 18.0, r.y + 3.0, 15.0, 14.0);
            draw_rectangle(x.x, x.y, x.w, x.h, Color::new(0.75, 0.25, 0.25, 1.0));
            txt_c("X", x.x + x.w / 2.0, x.y + 11.0, 10.0, WHITE);
            let corner = Rect::new(r.x + r.w - 12.0, r.y + r.h - 12.0, 12.0, 12.0);
            draw_triangle(vec2(r.x + r.w - 2.0, r.y + r.h - 12.0), vec2(r.x + r.w - 2.0, r.y + r.h - 2.0), vec2(r.x + r.w - 12.0, r.y + r.h - 2.0), Color::new(0.2, 0.3, 0.45, 1.0));
            if self.menu.is_none() && is_mouse_button_pressed(MouseButton::Left) && self.drag.is_none() {
                if x.contains(m) {
                    remove = Some(i);
                } else if corner.contains(m) {
                    start = Some(Drag::WidResize { i });
                } else if r.contains(m) {
                    start = Some(Drag::WidMove { i, off: m - vec2(r.x, r.y) });
                }
            }
        }
        if let Some(i) = remove {
            self.widgets.remove(i);
            self.save();
        } else if let Some(d) = start {
            self.drag = Some(d);
        }
        // Palette
        draw_rectangle(pal.x, pal.y, pal.w, pal.h, Color::new(0.12, 0.13, 0.15, 1.0));
        txt("ELEMENTS", pal.x + 8.0, pal.y + 16.0, 11.0, Color::new(0.7, 0.74, 0.78, 1.0));
        let ih = ((pal.h - 90.0) / ELEMS.len() as f32).min(26.0);
        for (k, (e, l)) in ELEMS.iter().enumerate() {
            let b = Rect::new(pal.x + 6.0, pal.y + 24.0 + k as f32 * ih, pal.w - 12.0, ih - 3.0);
            draw_rectangle(b.x, b.y, b.w, b.h, if hovered(b) { Color::new(0.2, 0.36, 0.5, 1.0) } else { Color::new(0.2, 0.22, 0.25, 1.0) });
            txt(l, b.x + 8.0, b.y + b.h / 2.0 + 4.0, 10.0, WHITE);
            if self.menu.is_none() && is_mouse_button_pressed(MouseButton::Left) && b.contains(m) && self.drag.is_none() {
                self.drag = Some(Drag::Palette { elem: *e });
            }
        }
        // Écran principal de la version normale : page composée ou disposition standard
        let mb = Rect::new(pal.x + 6.0, pal.y + pal.h - 60.0, pal.w - 12.0, 24.0);
        draw_rectangle(mb.x, mb.y, mb.w, mb.h, if self.main_screen { Color::new(0.1, 0.45, 0.3, 1.0) } else if hovered(mb) { Color::new(0.25, 0.27, 0.3, 1.0) } else { Color::new(0.2, 0.22, 0.25, 1.0) });
        txt_c(if self.main_screen { "ECRAN PRINCIPAL : PAGE COMPOSEE" } else { "ECRAN PRINCIPAL : STANDARD" }, mb.x + mb.w / 2.0, mb.y + 16.0, 9.0, WHITE);
        if self.menu.is_none() && clicked(mb) {
            self.main_screen = !self.main_screen;
            self.save();
        }
        let rb = Rect::new(pal.x + 6.0, pal.y + pal.h - 30.0, pal.w - 12.0, 24.0);
        draw_rectangle(rb.x, rb.y, rb.w, rb.h, if hovered(rb) { Color::new(0.5, 0.25, 0.2, 1.0) } else { Color::new(0.3, 0.2, 0.18, 1.0) });
        txt_c("DISPOSITION PAR DEFAUT", rb.x + rb.w / 2.0, rb.y + 16.0, 10.0, WHITE);
        if self.menu.is_none() && clicked(rb) {
            self.widgets = default_widgets();
            self.save();
        }
        // Élément en cours de glisser depuis la palette
        if let Some(Drag::Palette { elem }) = self.drag {
            let r = Rect::new(m.x - 60.0, m.y - 14.0, 120.0, 28.0);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.3, 0.55, 0.85, 0.8));
            txt_c(elem_name(elem), m.x, m.y + 4.0, 11.0, WHITE);
        }
    }

    /// Barre de menus (dessinée en dernier, au premier plan). `console` : ligne en cours de saisie
    /// (None si fermée), `hist` : dernières instructions, `status` : texte de droite.
    pub fn bar(&mut self, sw: f32, console: Option<&str>, hist: &[String], log: &[String], status: &str) -> BarOut {
        let mut out = BarOut::default();
        let bar = Rect::new(0.0, 0.0, sw, BAR_H);
        draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(1.0, 1.0, 1.0, 1.0));
        draw_line(0.0, BAR_H, sw, BAR_H, 1.0, Color::new(0.75, 0.77, 0.8, 1.0));
        let dark = Color::new(0.1, 0.12, 0.15, 1.0);
        txt("DARKSTAR  DEV", 12.0, 20.0, 14.0, Color::new(0.05, 0.3, 0.5, 1.0));
        let menus = ["FENETRES", "CONSOLE", "DISPOSITION", "SIMULATEUR"];
        let mut x = 150.0;
        let mut rects = Vec::new();
        for (k, m) in menus.iter().enumerate() {
            let w = txt_w(m, 12.0) + 24.0;
            let b = Rect::new(x, 3.0, w, BAR_H - 6.0);
            let on = self.menu == Some(k) || (k == 1 && console.is_some());
            draw_rectangle(b.x, b.y, b.w, b.h, if on { Color::new(0.85, 0.9, 0.96, 1.0) } else if hovered(b) { Color::new(0.93, 0.94, 0.96, 1.0) } else { WHITE });
            txt_c(m, b.x + b.w / 2.0, b.y + 16.0, 12.0, dark);
            if clicked(b) {
                if k == 1 {
                    out.console_open = Some(console.is_none());
                    self.menu = None;
                } else {
                    self.menu = if self.menu == Some(k) { None } else { Some(k) };
                }
            }
            rects.push(b);
            x += w + 4.0;
        }
        txt_r(status, sw - 12.0, 20.0, 11.0, dark);
        // Menus déroulants
        if let Some(k) = self.menu {
            let b = rects[k];
            let items: Vec<(String, bool)> = match k {
                0 => self.wins.iter().map(|w| (win_name(w.kind).to_string(), w.open)).collect(),
                2 => vec![("ENREGISTRER LA DISPOSITION".into(), false), ("DISPOSITION PAR DEFAUT".into(), false), ("PAGE 1 PAR DEFAUT".into(), false)],
                _ => vec![("QUITTER LA VERSION DEV".into(), false)],
            };
            let w = 240.0;
            let r = Rect::new(b.x, BAR_H, w, items.len() as f32 * 24.0 + 8.0);
            draw_rectangle(r.x + 3.0, r.y + 3.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.12));
            draw_rectangle(r.x, r.y, r.w, r.h, WHITE);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.75, 0.77, 0.8, 1.0));
            let mut chosen = None;
            for (i, (l, on)) in items.iter().enumerate() {
                let it = Rect::new(r.x + 4.0, r.y + 4.0 + i as f32 * 24.0, r.w - 8.0, 22.0);
                if hovered(it) {
                    draw_rectangle(it.x, it.y, it.w, it.h, Color::new(0.9, 0.94, 0.98, 1.0));
                }
                if k == 0 {
                    draw_rectangle_lines(it.x + 6.0, it.y + 5.0, 12.0, 12.0, 1.0, dark);
                    if *on {
                        draw_rectangle(it.x + 9.0, it.y + 8.0, 6.0, 6.0, Color::new(0.05, 0.4, 0.65, 1.0));
                    }
                }
                txt(l, it.x + 26.0, it.y + 15.0, 11.0, dark);
                if clicked(it) {
                    chosen = Some(i);
                }
            }
            if let Some(i) = chosen {
                match k {
                    0 => {
                        self.wins[i].open = !self.wins[i].open;
                        // Fenêtre rouverte : au premier plan
                        if self.wins[i].open {
                            let w = self.wins.remove(i);
                            self.wins.push(w);
                        }
                        self.save();
                    }
                    2 => {
                        match i {
                            1 => self.wins = default_wins(),
                            2 => self.widgets = default_widgets(),
                            _ => {}
                        }
                        self.save();
                        self.menu = None;
                    }
                    _ => {
                        out.quit = true;
                        self.menu = None;
                    }
                }
            } else if is_mouse_button_pressed(MouseButton::Left) && !r.contains(vec2(mouse_position().0, mouse_position().1)) && !rects.iter().any(|b| b.contains(vec2(mouse_position().0, mouse_position().1))) {
                self.menu = None;
            }
        }
        // Console : déroulant sous la barre (saisie, historique, derniers messages)
        if let Some(line) = console {
            let b = rects[1];
            let w = (sw * 0.45).max(520.0);
            let r = Rect::new(b.x, BAR_H, w, 230.0);
            draw_rectangle(r.x + 3.0, r.y + 3.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.15));
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.06, 0.07, 0.09, 0.97));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.3, 0.6, 0.85, 1.0));
            let cur = if (get_time() * 2.0).fract() < 0.5 { "_" } else { " " };
            txt(&format!("> {line}{cur}"), r.x + 10.0, r.y + 22.0, 14.0, Color::new(0.5, 0.9, 1.0, 1.0));
            txt("ENTREE : executer   ECHAP : fermer   FLECHE HAUT : historique", r.x + 10.0, r.y + 40.0, 10.0, Color::new(0.55, 0.6, 0.65, 1.0));
            let mut y = r.y + 60.0;
            for l in log.iter().rev().take(8).collect::<Vec<_>>().into_iter().rev() {
                txt(&l.chars().take(90).collect::<String>(), r.x + 10.0, y, 10.0, Color::new(0.8, 0.84, 0.88, 1.0));
                y += 14.0;
            }
            let _ = hist;
        }
        out
    }
}

/// Panneau droit : boutons-poussoirs moteurs (turbines, entrées d'air, stato, scram) et APU
/// (MASTER / START, GEN, BLEED), jauges de l'APU. Renvoie l'instruction à exécuter.
pub fn right_panel(ac: &crate::aircraft::Aircraft, r: Rect) -> Option<String> {
    use crate::aircraft::*;
    use crate::apu::ApuState;
    let mut out = None;
    let pad = 8.0;
    let lbl = Color::new(0.62, 0.66, 0.7, 1.0);
    // Bouton-poussoir lumineux : légende, voyant (vert ON / ambre / éteint)
    let mut button = |x: f32, y: f32, w: f32, title: &str, state: &str, lit: Option<Color>, cmd: String| {
        let b = Rect::new(x, y, w, 40.0);
        draw_rectangle(b.x, b.y, b.w, b.h, if hovered(b) { Color::new(0.16, 0.17, 0.2, 1.0) } else { Color::new(0.11, 0.12, 0.14, 1.0) });
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, Color::new(0.3, 0.32, 0.36, 1.0));
        txt_c(title, b.x + b.w / 2.0, b.y + 14.0, 10.0, WHITE);
        let (c, s) = match lit {
            Some(c) => (c, state),
            None => (Color::new(0.3, 0.32, 0.35, 1.0), state),
        };
        draw_rectangle(b.x + 6.0, b.y + 21.0, b.w - 12.0, 14.0, Color::new(0.0, 0.0, 0.0, 1.0));
        txt_c(s, b.x + b.w / 2.0, b.y + 32.0, 10.0, c);
        if clicked(b) {
            out = Some(cmd);
        }
    };
    let cols = 2.0;
    let bw = (r.w - pad * (cols + 1.0)) / cols;
    let x0 = r.x + pad;
    let x1 = x0 + bw + pad;
    let mut y = r.y + 8.0;
    txt("MOTEURS", r.x + pad, y + 10.0, 11.0, lbl);
    y += 16.0;
    let e = &ac.eng;
    let turb_on = matches!(e.turb, TurbState::Running | TurbState::Starting);
    let green = GREEN_S;
    button(x0, y, bw, "TURBINES", if e.turb == TurbState::Starting { "START" } else if turb_on { "ON" } else { "OFF" }, if turb_on { Some(green) } else { None }, if turb_on { "TURB STOP".into() } else { "TURB START".into() });
    button(x1, y, bw, "ENTREES D'AIR", if e.inlet_open { "OPEN" } else { "CLOSED" }, if e.inlet_open { Some(green) } else { Some(AMBER) }, if e.inlet_open { "INLET CLOSE".into() } else { "INLET OPEN".into() });
    y += 46.0;
    let (rs, rc, rcmd) = match e.ram {
        RamState::Lit => ("LIT", Some(green), "RAM OFF"),
        RamState::Armed => ("ARM", Some(CYAN_S), "RAM IGN"),
        _ => ("OFF", None, "RAM ARM"),
    };
    button(x0, y, bw, "STATO", rs, rc, rcmd.into());
    let (ss, sc, scmd) = match e.scram {
        ScramState::Lit => ("LIT", Some(green), "SCRAM OFF"),
        ScramState::Armed => ("ARM", Some(CYAN_S), "SCRAM IGN"),
        ScramState::Unstart => ("UNSTART", Some(AMBER), "SCRAM IGN"),
        _ => ("OFF", None, "SCRAM ARM"),
    };
    button(x1, y, bw, "SCRAM", ss, sc, scmd.into());
    y += 52.0;
    // APU
    txt("APU", r.x + pad, y + 10.0, 11.0, lbl);
    let a = &ac.apu;
    let st = match a.state {
        ApuState::Running if a.n > 0.95 => ("AVAIL", green),
        ApuState::Starting => ("STARTING", CYAN_S),
        ApuState::Stopping => ("COOLING", AMBER),
        _ => ("OFF", lbl),
    };
    txt_r(st.0, r.x + r.w - pad, y + 10.0, 11.0, st.1);
    y += 16.0;
    let running = matches!(a.state, ApuState::Starting | ApuState::Running);
    button(x0, y, bw, "MASTER / START", if running { "ON" } else { "OFF" }, if running { Some(green) } else { None }, if running { "APU STOP".into() } else { "APU START".into() });
    button(x1, y, bw, "APU GEN", if a.generator { if a.gen_online() { "ON LINE" } else { "ON" } } else { "OFF" }, if a.gen_online() { Some(green) } else if a.generator { Some(CYAN_S) } else { None }, if a.generator { "APU GEN OFF".into() } else { "APU GEN ON".into() });
    y += 46.0;
    button(x0, y, bw, "APU BLEED", if a.bleed { if a.bleed_air() { "AIR" } else { "ON" } } else { "OFF" }, if a.bleed_air() { Some(green) } else if a.bleed { Some(CYAN_S) } else { None }, if a.bleed { "APU BLEED OFF".into() } else { "APU BLEED ON".into() });
    // Jauges APU : N % et EGT
    let gx = x1;
    let gw = bw;
    let bar = |yy: f32, name: &str, v: f32, txtv: String, c: Color| {
        txt(name, gx, yy + 9.0, 10.0, lbl);
        txt_r(&txtv, gx + gw, yy + 9.0, 10.0, WHITE);
        draw_rectangle(gx, yy + 13.0, gw, 6.0, Color::new(0.18, 0.19, 0.22, 1.0));
        draw_rectangle(gx, yy + 13.0, gw * v.clamp(0.0, 1.0), 6.0, c);
    };
    bar(y, "N", a.n as f32, format!("{:.0} %", a.n * 100.0), green);
    let egt_c = if a.egt > 650.0 { AMBER } else { green };
    bar(y + 21.0, "EGT", (a.egt / 800.0) as f32, format!("{:.0} C", a.egt), egt_c);
    y += 52.0;
    if y + 14.0 < r.y + r.h {
        txt(&format!("CARBURANT APU {:.0} KG/H", a.ff * 3600.0), r.x + pad, y + 8.0, 10.0, lbl);
    }
    out
}

impl DevUi {
    /// Rectangles des fenêtres ouvertes au-dessus de la fenêtre `i`, de la barre et des menus
    /// déroulants : les clics n'y atteignent pas la fenêtre `i`.
    pub fn blockers_above(&self, i: usize, sw: f32, console_open: bool) -> Vec<Rect> {
        let mut v: Vec<Rect> = self.wins.iter().enumerate().filter(|(k, w)| *k > i && w.open).map(|(_, w)| self.rect_of(w)).collect();
        v.extend(self.overlays(sw, console_open));
        v
    }

    /// Barre de menus et menus déroulants ouverts.
    pub fn overlays(&self, sw: f32, console_open: bool) -> Vec<Rect> {
        let mut v = vec![Rect::new(0.0, 0.0, sw, BAR_H)];
        if self.menu.is_some() {
            v.push(Rect::new(140.0, BAR_H, 400.0, 260.0));
        }
        if console_open {
            v.push(Rect::new(140.0, BAR_H, (sw * 0.45).max(520.0) + 120.0, 232.0));
        }
        v
    }
}
