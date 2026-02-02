use std::fs;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::types::{OpencodeCommand, WorkspaceOpenworkConfig};
use crate::utils::now_ms;
use crate::workspace::commands::{sanitize_command_name, serialize_command_frontmatter};

pub fn merge_plugins(existing: Vec<String>, required: &[&str]) -> Vec<String> {
    let mut out = existing;
    for plugin in required {
        if !out.iter().any(|entry| entry == plugin) {
            out.push(plugin.to_string());
        }
    }
    out
}

fn seed_workspace_guide(skill_root: &PathBuf) -> Result<(), String> {
    let guide_dir = skill_root.join("workspace-guide");
    if guide_dir.exists() {
        return Ok(());
    }

    fs::create_dir_all(&guide_dir)
        .map_err(|e| format!("Failed to create {}: {e}", guide_dir.display()))?;

    let doc = r#"---
name: workspace-guide
description: Workspace guide to introduce OpenWork and onboard new users.
---

# Welcome to OpenWork

Hi, I'm Ben and this is OpenWork. It's an open-source alternative to Claude's cowork. It helps you work on your files with AI and automate the mundane tasks so you don't have to.

Before we start, use the question tool to ask:
"Are you more technical or non-technical? I'll tailor the explanation."

## If the person is non-technical
OpenWork feels like a chat app, but it can safely work with the files you allow. Put files in this workspace and I can summarize them, create new ones, or help organize them.

Try:
- "Summarize the files in this workspace."
- "Create a checklist for my week."
- "Draft a short summary from this document."

## Skills and plugins (simple)
Skills add new capabilities. Plugins add advanced features like scheduling or browser automation. We can add them later when you're ready.

## If the person is technical
OpenWork is a GUI for OpenCode. Everything that works in OpenCode works here.

Most reliable setup today:
1) Install OpenCode from opencode.ai
2) Configure providers there (models and API keys)
3) Come back to OpenWork and start a session

Skills:
- Install from the Skills tab, or add them to this workspace.
- Docs: https://opencode.ai/docs/skills

Plugins:
- Configure in opencode.json or use the Plugins tab.
- Docs: https://opencode.ai/docs/plugins/

MCP servers:
- Add external tools via opencode.json.
- Docs: https://opencode.ai/docs/mcp-servers/

Config reference:
- Docs: https://opencode.ai/docs/config/

End with two friendly next actions to try in OpenWork."#;

    fs::write(guide_dir.join("SKILL.md"), doc)
        .map_err(|e| format!("Failed to write SKILL.md: {e}"))?;

    Ok(())
}

fn seed_get_started_skill(skill_root: &PathBuf) -> Result<(), String> {
  let skill_dir = skill_root.join("get-started");
    if skill_dir.exists() {
        return Ok(());
    }

    fs::create_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to create {}: {e}", skill_dir.display()))?;

  let doc = r#"---
name: get-started
description: Guide users through the get started setup and Chrome DevTools demo.
---

## When to use
- Always load this skill when the user says \"get started\".

## What to do
- Reply with these four lines, exactly and in order:
  1) hey there welcome this is openwork
  2) we've pre-configured you with a couple tools
  3) Get Started
  4) write \"hey go on google.com\"

## Then
- If the user writes \"go on google.com\" (or \"hey go on google.com\"), use the chrome-devtools MCP to open the site.
- After the navigation completes, reply: \"I'm on <site>\" where <site> is the final URL or page title they asked for.
"#;

    fs::write(skill_dir.join("SKILL.md"), doc)
        .map_err(|e| format!("Failed to write SKILL.md: {e}"))?;

    Ok(())
}

const ENTERPRISE_ARCHIVE_URL: &str =
    "https://github.com/different-ai/openwork-enterprise/archive/refs/heads/main.zip";
const ENTERPRISE_SEED_MARKER: &str = ".openwork-enterprise-creators";

fn seed_enterprise_creator_skills(root: &PathBuf, skill_root: &PathBuf) -> Result<(), String> {
    let marker_path = root.join(".opencode").join(ENTERPRISE_SEED_MARKER);
    if marker_path.exists() {
        return Ok(());
    }

    let mut existing = HashSet::new();
    if let Ok(entries) = fs::read_dir(skill_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.is_empty() {
                existing.insert(name);
            }
        }
    }

    let agent = ureq::AgentBuilder::new().redirects(5).build();
    let response = agent
        .get(ENTERPRISE_ARCHIVE_URL)
        .call()
        .map_err(|e| format!("Failed to download enterprise archive: {e}"))?;

    let mut buffer = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut buffer)
        .map_err(|e| format!("Failed to read enterprise archive: {e}"))?;

    let cursor = Cursor::new(buffer);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| format!("Failed to open enterprise archive: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read enterprise entry: {e}"))?;
        let name = entry.name().to_string();
        let entry_path = Path::new(&name);
        if entry_path.components().any(|component| match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => true,
            _ => false,
        }) {
            continue;
        }

        let parts: Vec<String> = entry_path
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect();
        if parts.len() < 5 {
            continue;
        }
        if parts[1] != ".opencode" || parts[2] != "skills" {
            continue;
        }

        let skill_name = &parts[3];
        if !skill_name.ends_with("-creator") {
            continue;
        }
        if existing.contains(skill_name) {
            continue;
        }

        let dest_root = skill_root.join(skill_name);
        let mut dest_path = dest_root.clone();
        for part in parts.iter().skip(4) {
            dest_path = dest_path.join(part);
        }

        if name.ends_with('/') {
            fs::create_dir_all(&dest_path)
                .map_err(|e| format!("Failed to create {}: {e}", dest_path.display()))?;
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }

        let mut file_buffer = Vec::new();
        entry
            .read_to_end(&mut file_buffer)
            .map_err(|e| format!("Failed to read enterprise entry: {e}"))?;
        fs::write(&dest_path, file_buffer)
            .map_err(|e| format!("Failed to write {}: {e}", dest_path.display()))?;
    }

    fs::write(&marker_path, "seeded\n")
        .map_err(|e| format!("Failed to write {}: {e}", marker_path.display()))?;

    Ok(())
}

fn seed_commands(commands_dir: &PathBuf, preset: &str) -> Result<(), String> {
  if fs::read_dir(commands_dir)
    .map_err(|e| format!("Failed to read {}: {e}", commands_dir.display()))?
    .next()
    .is_some()
  {
    return Ok(());
  }

  let defaults = vec![
    OpencodeCommand {
      name: "learn-files".to_string(),
      description: Some("Safe, practical file workflows".to_string()),
      template: "Show me how to interact with files in this workspace. Include safe examples for reading, summarizing, and editing.".to_string(),
      agent: None,
      model: None,
      subtask: None,
    },
    OpencodeCommand {
      name: "learn-skills".to_string(),
      description: Some("How skills work and how to create your own".to_string()),
      template: "Explain what skills are, how to use them, and how to create a new skill for this workspace.".to_string(),
      agent: None,
      model: None,
      subtask: None,
    },
    OpencodeCommand {
      name: "learn-plugins".to_string(),
      description: Some("What plugins are and how to install them".to_string()),
      template: "Explain what plugins are and how to install them in this workspace.".to_string(),
      agent: None,
      model: None,
      subtask: None,
    },
  ];

  let mut defaults = defaults;
  if preset == "starter" {
    defaults.push(OpencodeCommand {
      name: "Get Started".to_string(),
      description: Some("Get started".to_string()),
      template: "get started".to_string(),
      agent: None,
      model: None,
      subtask: None,
    });
  }

    for command in defaults {
        let Some(name) = sanitize_command_name(&command.name) else {
            continue;
        };

    let file_path = commands_dir.join(format!("{name}.md"));
    if file_path.exists() {
      continue;
    }

    let serialized = serialize_command_frontmatter(&command)?;
    fs::write(&file_path, serialized)
      .map_err(|e| format!("Failed to write {}: {e}", file_path.display()))?;
  }

    Ok(())
}

fn seed_agents(agents_dir: &PathBuf, preset: &str) -> Result<(), String> {
    // Ne seed que pour le preset "starter"
    if preset != "starter" {
        return Ok(());
    }

    // Vérifier si le dossier est déjà peuplé
    if fs::read_dir(agents_dir)
        .ok()
        .and_then(|mut d| d.next())
        .is_some()
    {
        return Ok(());
    }

    fs::create_dir_all(agents_dir)
        .map_err(|e| format!("Failed to create agents dir: {e}"))?;

    // Agent contextualisation-aav
    let contextualisation_aav_content = r#"---
description: Agent de Contextualisation Avant-Vente pour IPPON Technologies
mode: primary
model: gpt-5.2-codex
tools:
  write: false
  edit: false
  bash: false
---
Tu es l'Agent de Contextualisation Avant-Vente pour IPPON Technologies.
Ton rôle est d'analyser les documents d'entrée (Appels d'offres, emails, notes) pour produire une analyse initiale structurée.

<INSTRUCTIONS>

### TA SOURCE DE VÉRITÉ (CRITIQUE)
Tu disposes en contexte du **"Contexte Système : Les Offres Stratégiques IPPON"** situé <CONTEXTE_METIER> à la fin du fichier. Tu dois consulter cette section automatiquement pour chaque réponse.
Tu ne dois **jamais** inventer de définitions. Tu dois évaluer l'opportunité strictement à travers les sections de ce document :
1.  **Offre Modernisation du SI** (Référence : Section 1 du document)
2.  **Offre Plateformisation** (Référence : Section 2 du document)
3.  **Offre Intelligence Artificielle** (Référence : Section 3 du document)

### TES TÂCHES
1.  **Synthétiser le Contexte** : Résume le besoin client, les enjeux business et les contraintes.
2.  **Qualifier l'Opportunité** :
    * Compare les mots du client avec la section **"Signaux Clients & Pain Points"** du document référence.
    * Vérifie systématiquement la présence de **"Contradictions / Points de Vigilance (Anti-Patterns)"** listés dans le document.
3.  **Inférer les Compétences** : Déduis les compétences techniques et méthodologiques requises. Utilise la section "Synthèse des Compétences Transverses" pour guider tes choix (ex: DevOps pour lier IA et Plateforme).
4.  **Scorer (Go / No-Go)** : Évalue l'alignement avec les sections **"La Réponse IPPON"** (ex: respect de la règle 70% Métier pour l'IA, approche Vision 360 pour la modernisation).

### FORMAT DE SORTIE ATTENDU
Tu dois toujours répondre avec cette structure exacte :

## 1. Synthèse Exécutive
* **Client & Secteur** : [Nom] - [Secteur]
* **Le Besoin (Pitch)** : [Résumé en 2 phrases]
* **Enjeux Business** : [Liste des douleurs/objectifs clés]

## 2. Grille de Qualification IPPON
| Critère | Analyse (Basée sur le document référence) |
| :--- | :--- |
| **Offre IPPON Principale** | [Modernisation / Plateformisation / IA] |
| **Signaux Détectés** | [Quels "Signaux Clients" du document référence matchent avec la demande ?] |
| **Points de Vigilance** | [Y a-t-il un "Anti-Pattern" du document ? Ex: "IA sans Métier", "Outil vs Produit"...] |
| **Alignement Convictions** | [Faible / Moyen / Fort] - Justifier en citant une "Conviction Forte" ou un "Bénéfice Clé" du document. |

## 3. Cartographie des Compétences (Inférence)
* **Domaines d'expertise** : [Ex: Cloud, Data, Java, Agile...]
* **Rôles Clés suggérés** : [Ex: Tech Lead, Product Owner, Data Engineer]
* **Stack Technique déduite** : [Lister les technos explicites + celles implicites]

## 4. Recommandation Préliminaire
**AVIS : [GO / NO-GO / NURTURE]**
* **Pourquoi ?** : [Argumentaire basé sur la conformité aux convictions IPPON]
* **Questions manquantes critiques** : [Ce qui manque pour décider, ex: Budget, Timeline, Accès aux utilisateurs finaux]

</INSTRUCTIONS>

<CONTEXTE_METIER>

### Contexte Système : Les Offres Stratégiques IPPON
Ce document détaille les trois piliers de l'offre de valeur d'Ippon Technologies. Il doit servir de référence pour évaluer la pertinence des demandes clients, aligner les propositions de valeur et qualifier les opportunités.
1. Offre Modernisation du SI (Système d'Information)
Définition et Ambition
La modernisation du SI ne se limite pas à des choix technologiques ; c'est une transformation 360° visant à accélérer la transformation digitale. L'objectif est de passer d'un mode défensif (dette technique, pannes) à un mode créateur de valeur.
Signaux Clients & Pain Points (Opportunités)
Le client est une cible pour cette offre s'il mentionne :
• "Éteindre le feu" : Taux d'erreurs élevé, paralysie de l'innovation par la maintenance.
• Charge cognitive trop lourde : Applications monolithiques ("boîte à tout faire") que plus personne n'ose toucher.
• Lenteur du Delivery : Cycles de déploiement très longs (ex: "la moindre évolution demande 9 mois de planification").
• Insatisfaction Métier : UX pauvre, manque de réactivité face aux demandes d'évolution, sentiment de ne pas être écouté.
La Réponse IPPON (Valeur Ajoutée)
• Approche Holistique (Vision 360) : Ippon intervient sur 4 axes simultanés : Produit, Technologie, Culture & Organisation, et Expérientielle.
• Bénéfices Clés :
    ◦ Répondre aux exigences métiers en créant une valeur différenciante.
    ◦ Gagner en réactivité et agilité (réduire le Time-to-Market).
    ◦ Reprendre le contrôle des budgets et des roadmaps.
• Méthodologie : Démarche structurée en 5 étapes :
    1. Périmètre, Ambition & Vision.
    2. Analyse de l'existant (plans fonctionnels, applicatifs, techniques).
    3. Définition de la cible (scénarios, architecture).
    4. Définition de la trajectoire (Roadmap, chiffrage, risques).
    5. Accompagnement à la modernisation continue.
Contradictions / Points de Vigilance (Anti-Patterns)
• Le client veut moderniser uniquement la "tech" sans toucher à l'organisation ou aux processus. (Contradiction avec la vision 360).
• Le client souhaite conserver des architectures fortement couplées tout en exigeant de l'agilité..
--------------------------------------------------------------------------------
2. Offre Plateformisation (Platform Engineering)
Définition et Ambition
La plateformisation transforme les capacités IT en plateformes réutilisables et évolutives. Elle vise à structurer un système IT interconnecté mais désorganisé pour orchestrer les services (internes/externes) et offrir une expérience cohérente. C'est un levier de productivité et d'innovation.
Signaux Clients & Pain Points (Opportunités)
Le client est une cible pour cette offre s'il cherche à :
• Maîtriser ses coûts opérationnels et réduire les doublons,.
• Améliorer l'expérience développeur (DevX).
• Passer à l'échelle sans ajouter de complexité aux processus internes.
• Booster sa capacité de production grâce à l'IA.
La Réponse IPPON (Valeur Ajoutée)
• Types de Plateformes couvertes :
    ◦ Plateforme de Développement (Outils de build, test, deploy).
    ◦ Plateforme IA (MLOps, automatisation du cycle de vie des modèles).
    ◦ Plateforme Data & Dataviz (Collecte, stockage, analyse).
    ◦ Plateforme Infrastructure (Gestion centralisée Cloud/On-premise).
• Métriques de Succès (Preuves) :
    ◦ -25% de Lead Time.
    ◦ +50% de déploiements par semestre.
    ◦ -30% de MTTR (Mean Time To Recover) sur les incidents.
    ◦ Time-to-environment < 1 heure.
• Méthodologie : De l'audit ("Formaliser vos ambitions") à l'opération ("Managed Services 24/7"), en passant par la construction et la formation des équipes (coaching pour acquérir les compétences techniques).
Contradictions / Points de Vigilance (Anti-Patterns)
• Le client voit la plateforme uniquement comme une suite d'outils. (Ippon insiste : c'est avant tout une transformation des processus internes).
• Négliger l'approche "Produit" : La plateforme doit être gérée comme un produit avec une interface simple et intuitive pour ses utilisateurs (les développeurs).
--------------------------------------------------------------------------------
3. Offre Intelligence Artificielle (IA)
Définition et Ambition
L'IA chez Ippon s'inscrit dans l'ère cognitive et la "Positive Technology". L'objectif est d'automatiser des segments métiers précis en réduisant l'incertitude inhérente aux projets IA.
Signaux Clients & Pain Points (Opportunités)
Le client est une cible pour cette offre s'il exprime :
• Une incertitude sur la valeur ou la faisabilité de ses cas d'usage IA ("Incertitude inhérente").
• Des craintes liées à la régulation (AI Act août 2026), à l'éthique ou aux coûts imprévus (FinOps).
• Un besoin de passer à l'échelle après des POCs (Proof of Concept) sans lendemain (besoin de MLOps).
La Réponse IPPON (Valeur Ajoutée)
• Convictions Fortes :
    ◦ "10% d'IA, 20% de Data, 70% de Métier" : La technologie n'est rien sans cas d'usage métier et data de qualité.
    ◦ Clean Code & IA : Le code généré par IA impose des standards de qualité et de lisibilité encore plus élevés ("Aide-toi et l'IA t'aidera").
• Méthode "Discovery to Delivery", :
    1. AI Discovery Sprint : Cadrage, idéation, roadmap (Réduire l'incertitude).
    2. AI MVP : Implémentation réelle ("Quick Win & Fail Fast").
    3. AI Scale : Industrialisation et déploiement à l'échelle.
• Accélérateurs : Capacité à déployer un premier chatbot en moins de deux jours sur une landing zone AWS via la structure IALab.
• Expertise MLOps & Compliance : Mutualisation des prérequis (monitoring, versioning) via une stratégie de plateforme.
Contradictions / Points de Vigilance (Anti-Patterns)
• Le client veut faire de l'IA sans impliquer le métier ou sans audit de la maturité Data. (Contredit la règle des 70% métier).
• Le client néglige la qualité du code sous prétexte que l'IA "code toute seule". (Ippon insiste sur un code "lisible et de haute qualité" nécessaire pour la maintenabilité future).
• Lancement de projets IA sans réflexion FinOps ou Éthique. (Risque face à l'AI Act et aux coûts écologiques/financiers).
--------------------------------------------------------------------------------
Synthèse des Compétences Transverses
Pour valider la crédibilité technique, Ippon s'appuie sur des certifications majeures (AWS, Azure, Google Cloud, OVHcloud). Un point d'attention particulier doit être porté à la "DevOps Services Competency" (AWS), qui est un pilier transversal reliant l'offre IA (pour le MLOps/CI/CD) et l'offre Plateformisation (pour l'ingénierie de plateforme).

</CONTEXTE_METIER>
"#;

    let file_path = agents_dir.join("contextualisation-aav.md");
    fs::write(&file_path, contextualisation_aav_content)
        .map_err(|e| format!("Failed to write contextualisation-aav.md: {e}"))?;

    Ok(())
}

pub fn ensure_workspace_files(workspace_path: &str, preset: &str) -> Result<(), String> {
    let root = PathBuf::from(workspace_path);

    let skill_root = root.join(".opencode").join("skills");
    fs::create_dir_all(&skill_root)
        .map_err(|e| format!("Failed to create .opencode/skills: {e}"))?;
    seed_workspace_guide(&skill_root)?;
  if preset == "starter" {
    seed_get_started_skill(&skill_root)?;
    if let Err(err) = seed_enterprise_creator_skills(&root, &skill_root) {
      println!("[workspace] Failed to seed creator skills: {err}");
    }
  }

    let commands_dir = root.join(".opencode").join("commands");
    fs::create_dir_all(&commands_dir)
        .map_err(|e| format!("Failed to create .opencode/commands: {e}"))?;
  seed_commands(&commands_dir, preset)?;

    let agents_dir = root.join(".opencode").join("agents");
    fs::create_dir_all(&agents_dir)
        .map_err(|e| format!("Failed to create .opencode/agents: {e}"))?;
    seed_agents(&agents_dir, preset)?;

    let config_path_jsonc = root.join("opencode.jsonc");
    let config_path_json = root.join("opencode.json");
    let config_path = if config_path_jsonc.exists() {
        config_path_jsonc
    } else if config_path_json.exists() {
        config_path_json
    } else {
        config_path_jsonc
    };

    let config_exists = config_path.exists();
    let mut config_changed = !config_exists;
    let mut config: serde_json::Value = if config_exists {
        let raw = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read {}: {e}", config_path.display()))?;
        json5::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({
          "$schema": "https://opencode.ai/config.json"
        })
    };

    if !config.is_object() {
        config = serde_json::json!({
          "$schema": "https://opencode.ai/config.json"
        });
        config_changed = true;
    }

    let required_plugins: Vec<&str> = match preset {
        "starter" => vec!["opencode-scheduler"],
        "automation" => vec!["opencode-scheduler"],
        _ => vec![],
    };

    let should_seed_chrome_mcp = matches!(preset, "starter");

    if !required_plugins.is_empty() {
        let plugins_value = config
            .get("plugin")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));

        let existing_plugins: Vec<String> = match plugins_value {
            serde_json::Value::Array(arr) => arr
                .into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            serde_json::Value::String(s) => vec![s],
            _ => vec![],
        };

        let merged = merge_plugins(existing_plugins.clone(), &required_plugins);
        if merged != existing_plugins {
            config_changed = true;
        }
        if let Some(obj) = config.as_object_mut() {
            obj.insert(
                "plugin".to_string(),
                serde_json::Value::Array(
                    merged.into_iter().map(serde_json::Value::String).collect(),
                ),
            );
        }
    }

    if should_seed_chrome_mcp {
        if let Some(obj) = config.as_object_mut() {
            let mcp_value = obj
                .get("mcp")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            let mut mcp_obj = match mcp_value {
                serde_json::Value::Object(map) => map,
                _ => serde_json::Map::new(),
            };

            if !mcp_obj.contains_key("chrome-devtools") {
                mcp_obj.insert(
                    "chrome-devtools".to_string(),
                    serde_json::json!({
                      "type": "local",
                      "command": ["npx", "-y", "chrome-devtools-mcp@latest"]
                    }),
                );
                config_changed = true;
            }

            obj.insert("mcp".to_string(), serde_json::Value::Object(mcp_obj));
        }
    }

    if config_changed {
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("Failed to write {}: {e}", config_path.display()))?;
    }

    let openwork_path = root.join(".opencode").join("openwork.json");
    if !openwork_path.exists() {
        let openwork = WorkspaceOpenworkConfig::new(workspace_path, preset, now_ms());

        fs::create_dir_all(openwork_path.parent().unwrap())
            .map_err(|e| format!("Failed to create {}: {e}", openwork_path.display()))?;

        fs::write(
            &openwork_path,
            serde_json::to_string_pretty(&openwork).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("Failed to write {}: {e}", openwork_path.display()))?;
    }

    Ok(())
}
