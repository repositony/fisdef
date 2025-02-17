// intenral
use crate::cli::SortProperty;

// Neutronics toolbox
use ntools::iaea::{self, IsomerState, Record, RecordSet};
use ntools::utils::OptionExt;

// external
use anyhow::Result;
use log::{debug, trace};
use serde::ser::{Serialize, SerializeStruct, Serializer};

#[derive(Debug, Clone)]
pub struct Source {
    pub fispact_name: String,
    pub fispact_activity: f64,
    pub iaea_nuclide: iaea::Nuclide,
    pub iaea_records: RecordSet,
}

impl Serialize for Source {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Create a struct serializer
        let mut state = serializer.serialize_struct("Source", 5)?;

        state.serialize_field("name_fispact", &self.fispact_name)?;
        state.serialize_field("name_iaea", &self.iaea_nuclide.name_with_state())?;
        state.serialize_field("activity", &self.fispact_activity)?;

        let energy: Vec<Option<f32>> = self.iaea_records.iter().map(|r| r.energy).collect();
        let intensity: Vec<Option<f32>> = self.iaea_records.iter().map(|r| r.intensity).collect();

        state.serialize_field("energy", &energy)?;
        state.serialize_field("intensity", &intensity)?;

        state.end()
    }
}

impl PartialEq for Source {
    fn eq(&self, other: &Self) -> bool {
        self.fispact_name == other.fispact_name && self.iaea_nuclide == other.iaea_nuclide
    }
}

impl Source {
    /// Normalisation factor for the decay data
    pub fn norm(&self) -> f64 {
        (self
            .iaea_records
            .iter()
            .fold(0.0, |acc, r| acc + r.intensity.unwrap_or(0.0))
            / 100.0) as f64
    }

    /// todo: Big mess of edge cases that neads cleaning up
    pub fn find_records(&mut self, radtype: iaea::RadType, fetch: bool) {
        let nuclide_records = match fetch {
            false => iaea::load_nuclide(self.iaea_nuclide.clone(), radtype),
            true => iaea::fetch_nuclide(self.iaea_nuclide.clone(), radtype),
        };

        if nuclide_records.is_none() {
            trace!("{radtype:?} decay records for {}: 0", self.fispact_name,);
            return;
        }

        if let Some(records) = nuclide_records {
            // get the list of parent energies
            let mut parent_energy = records
                .iter()
                .filter_map(|r| r.p_energy)
                .collect::<Vec<f32>>();
            parent_energy.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
            parent_energy.dedup();

            // get the index of the parent energy we care about
            let index = if let IsomerState::Excited(i) = self.iaea_nuclide.state {
                i as usize
            } else {
                0
            };

            let n = parent_energy.len();

            let target = if parent_energy[0] == 0.0 {
                if index >= n {
                    trace!(
                        "No {:?} records for excied state of {}",
                        radtype,
                        self.iaea_nuclide.name_with_state()
                    );
                    return;
                }

                parent_energy[index]
            } else {
                trace!(
                    "Note that {} records do not include a ground state",
                    self.iaea_nuclide.name()
                );

                if index == 0 {
                    trace!(
                        "No {:?} records for the ground state of {}",
                        radtype,
                        self.iaea_nuclide.name_with_state()
                    );
                    return;
                }

                // assume the first record is the first excited state
                trace!(
                    "Assuming {} keV is the first excited state of {}",
                    parent_energy[0],
                    self.iaea_nuclide.name()
                );

                if index > n {
                    trace!(
                        "No {:?} records for excied state of {}",
                        radtype,
                        self.iaea_nuclide.name_with_state()
                    );
                    return;
                }

                parent_energy[index - 1]
            };

            self.iaea_records = records
                .into_iter()
                .filter(|r| {
                    if let Some(e) = r.p_energy {
                        e == target
                    } else {
                        trace!("Unknown parent energy for {}", r.parent_name());
                        true
                    }
                })
                .collect::<Vec<Record>>();

            trace!(
                "{radtype:?} decay records for {}: {}",
                self.fispact_name,
                self.iaea_records.len(),
            );
        }
    }

    /// Remove unobserved records
    pub fn remove_unobserved_records(&mut self) {
        let n = self.iaea_records.len();
        self.iaea_records
            .retain(|r| match r.energy.is_some() && r.intensity.is_some() {
                true => true,
                false => {
                    trace!(
                        "Skipping bad {} record: \"{}\" keV, \"{}\" %",
                        self.fispact_name,
                        r.energy.display(),
                        r.intensity.display()
                    );
                    false
                }
            });

        if n != self.iaea_records.len() {
            debug!(
                "Records with unobserved emissions removed from {}",
                self.fispact_name
            );
        }
    }

    /// Sort records
    pub fn sort_records(&mut self, property: &SortProperty) {
        match property {
            SortProperty::Energy => {
                self.iaea_records
                    .sort_by(|a, b| a.energy.unwrap().partial_cmp(&b.energy.unwrap()).unwrap());
            }
            SortProperty::Intensity => {
                self.iaea_records.sort_by(|a, b| {
                    b.intensity
                        .unwrap()
                        .partial_cmp(&a.intensity.unwrap())
                        .unwrap()
                });
            }
        }
    }
}
