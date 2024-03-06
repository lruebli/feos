use crate::epcsaft::association::{AssociationParameters, AssociationRecord, BinaryAssociationRecord};
use crate::epcsaft::hard_sphere::{HardSphereProperties, MonomerShape};
use feos_core::parameter::{FromSegments, Parameter, ParameterError, PureRecord};
use ndarray::{Array, Array1, Array2};
use num_dual::DualNum;
use num_traits::Zero;
use feos_core::si::{JOULE, KB, KELVIN};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::Write;

use crate::epcsaft::eos::permittivity::PermittivityRecord;

/// PC-SAFT pure-component parameters.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ElectrolytePcSaftRecord {
    /// Segment number
    pub m: f64,
    /// Segment diameter in units of Angstrom
    pub sigma: f64,
    /// Energetic parameter in units of Kelvin
    pub epsilon_k: f64,
    /// Dipole moment in units of Debye
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mu: Option<f64>,
    /// Quadrupole moment in units of Debye
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<f64>,
    /// Association parameters
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub association_record: Option<AssociationRecord>,
    /// Entropy scaling coefficients for the viscosity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viscosity: Option<[f64; 4]>,
    /// Entropy scaling coefficients for the diffusion coefficient
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diffusion: Option<[f64; 5]>,
    /// Entropy scaling coefficients for the thermal conductivity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thermal_conductivity: Option<[f64; 4]>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permittivity_record: Option<PermittivityRecord>,
}

impl FromSegments<f64> for ElectrolytePcSaftRecord {
    fn from_segments(segments: &[(Self, f64)]) -> Result<Self, ParameterError> {
        let mut m = 0.0;
        let mut sigma3 = 0.0;
        let mut epsilon_k = 0.0;
        let mut z = 0.0;

        segments.iter().for_each(|(s, n)| {
            m += s.m * n;
            sigma3 += s.m * s.sigma.powi(3) * n;
            epsilon_k += s.m * s.epsilon_k * n;
            z += s.z.unwrap_or(0.0);
        });

        let q = segments
            .iter()
            .filter_map(|(s, n)| s.q.map(|q| q * n))
            .reduce(|a, b| a + b);
        let mu = segments
            .iter()
            .filter_map(|(s, n)| s.mu.map(|mu| mu * n))
            .reduce(|a, b| a + b);
        let association_record = segments
            .iter()
            .filter_map(|(s, n)| {
                s.association_record.as_ref().map(|record| {
                    [
                        record.kappa_ab * n,
                        record.epsilon_k_ab * n,
                        record.na * n,
                        record.nb * n,
                        record.nc * n,
                    ]
                })
            })
            .reduce(|a, b| {
                [
                    a[0] + b[0],
                    a[1] + b[1],
                    a[2] + b[2],
                    a[3] + b[3],
                    a[4] + b[4],
                ]
            })
            .map(|[kappa_ab, epsilon_k_ab, na, nb, nc]| {
                AssociationRecord::new(kappa_ab, epsilon_k_ab, na, nb, nc)
            });

        // entropy scaling
        let mut viscosity = if segments
            .iter()
            .all(|(record, _)| record.viscosity.is_some())
        {
            Some([0.0; 4])
        } else {
            None
        };
        let mut thermal_conductivity = if segments
            .iter()
            .all(|(record, _)| record.thermal_conductivity.is_some())
        {
            Some([0.0; 4])
        } else {
            None
        };
        let diffusion = if segments
            .iter()
            .all(|(record, _)| record.diffusion.is_some())
        {
            Some([0.0; 5])
        } else {
            None
        };

        let n_t = segments.iter().fold(0.0, |acc, (_, n)| acc + n);
        segments.iter().for_each(|(s, n)| {
            let s3 = s.m * s.sigma.powi(3) * n;
            if let Some(p) = viscosity.as_mut() {
                let [a, b, c, d] = s.viscosity.unwrap();
                p[0] += s3 * a;
                p[1] += s3 * b / sigma3.powf(0.45);
                p[2] += n * c;
                p[3] += n * d;
            }
            if let Some(p) = thermal_conductivity.as_mut() {
                let [a, b, c, d] = s.thermal_conductivity.unwrap();
                p[0] += n * a;
                p[1] += n * b;
                p[2] += n * c;
                p[3] += n_t * d;
            }
            // if let Some(p) = diffusion.as_mut() {
            //     let [a, b, c, d, e] = s.diffusion.unwrap();
            //     p[0] += s3 * a;
            //     p[1] += s3 * b / sigma3.powf(0.45);
            //     p[2] += *n * c;
            //     p[3] += *n * d;
            // }
        });
        // correction due to difference in Chapman-Enskog reference between GC and regular formulation.
        viscosity = viscosity.map(|v| [v[0] - 0.5 * m.ln(), v[1], v[2], v[3]]);

        Ok(Self {
            m,
            sigma: (sigma3 / m).cbrt(),
            epsilon_k: epsilon_k / m,
            mu,
            q,
            association_record,
            viscosity,
            diffusion,
            thermal_conductivity,
            z: Some(z),
            permittivity_record: None,
        })
    }
}

impl FromSegments<usize> for ElectrolytePcSaftRecord {
    fn from_segments(segments: &[(Self, usize)]) -> Result<Self, ParameterError> {
        // We do not allow more than a single segment for q, mu, kappa_ab, epsilon_k_ab    
        let segments: Vec<_> = segments
            .iter()
            .cloned()
            .map(|(s, c)| (s, c as f64))
            .collect();
        Self::from_segments(&segments)
    }
}
      

impl std::fmt::Display for ElectrolytePcSaftRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ElectrolytePcSaftRecord(m={}", self.m)?;
        write!(f, ", sigma={}", self.sigma)?;
        write!(f, ", epsilon_k={}", self.epsilon_k)?;
        if let Some(n) = &self.mu {
            write!(f, ", mu={}", n)?;
        }
        if let Some(n) = &self.q {
            write!(f, ", q={}", n)?;
        }
        if let Some(n) = &self.association_record {
            write!(f, ", association_record={}", n)?;
        }
        if let Some(n) = &self.viscosity {
            write!(f, ", viscosity={:?}", n)?;
        }
        if let Some(n) = &self.diffusion {
            write!(f, ", diffusion={:?}", n)?;
        }
        if let Some(n) = &self.thermal_conductivity {
            write!(f, ", thermal_conductivity={:?}", n)?;
        }
        if let Some(n) = &self.z {
            write!(f, ", z={}", n)?;
        }
        if let Some(n) = &self.permittivity_record {
            write!(f, ", permittivity_record={:?}", n)?;
        }
        write!(f, ")")
    }
}

impl ElectrolytePcSaftRecord {
    pub fn new(
        m: f64,
        sigma: f64,
        epsilon_k: f64,
        mu: Option<f64>,
        q: Option<f64>,
        kappa_ab: Option<f64>,
        epsilon_k_ab: Option<f64>,
        na: Option<f64>,
        nb: Option<f64>,
        nc: Option<f64>,
        viscosity: Option<[f64; 4]>,
        diffusion: Option<[f64; 5]>,
        thermal_conductivity: Option<[f64; 4]>,
        z: Option<f64>,
        permittivity_record: Option<PermittivityRecord>,
    ) -> ElectrolytePcSaftRecord {
        let association_record = if kappa_ab.is_none()
            && epsilon_k_ab.is_none()
            && na.is_none()
            && nb.is_none()
            && nc.is_none()
        {
            None
        } else {
            Some(AssociationRecord::new(
                kappa_ab.unwrap_or_default(),
                epsilon_k_ab.unwrap_or_default(),
                na.unwrap_or_default(),
                nb.unwrap_or_default(),
                nc.unwrap_or_default(),
            ))
        };
        ElectrolytePcSaftRecord {
            m,
            sigma,
            epsilon_k,
            mu,
            q,
            association_record,
            viscosity,
            diffusion,
            thermal_conductivity,
            z,
            permittivity_record,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ElectrolytePcSaftBinaryRecord {
    /// Binary dispersion interaction parameter
    #[serde(default)]
    pub k_ij: Vec<f64>,
    /// Binary association parameters
    #[serde(flatten)]
    association: Option<BinaryAssociationRecord>,
}

impl ElectrolytePcSaftBinaryRecord {
    pub fn new(k_ij: Option<Vec<f64>>, kappa_ab: Option<f64>, epsilon_k_ab: Option<f64>) -> Self {
        let k_ij = k_ij.unwrap_or_default();
        let association = if kappa_ab.is_none() && epsilon_k_ab.is_none() {
            None
        } else {
            Some(BinaryAssociationRecord::new(kappa_ab, epsilon_k_ab, None))
        };
        Self { k_ij, association }
    }
}

impl TryFrom<f64> for ElectrolytePcSaftBinaryRecord {
    type Error = ParameterError;

    fn try_from(k_ij: f64) -> Result<Self, Self::Error> {
        Ok(Self {
            k_ij: vec![k_ij, 0., 0., 0.],
            association: None,
        })
    }
}

impl TryFrom<ElectrolytePcSaftBinaryRecord> for f64 {
    type Error = ParameterError;

    fn try_from(_f: ElectrolytePcSaftBinaryRecord) -> Result<Self, Self::Error> {
        Err(ParameterError::IncompatibleParameters(
            "Cannot infer k_ij from single float.".to_string(),
        ))
    }
}

impl std::fmt::Display for ElectrolytePcSaftBinaryRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut tokens = vec![];
        if !self.k_ij[0].is_zero() {
            tokens.push(format!("ElectrolytePcSaftBinaryRecord(k_ij_0={})", self.k_ij[0]));
            tokens.push(format!("ElectrolytePcSaftBinaryRecord(k_ij_1={})", self.k_ij[1]));
            tokens.push(format!("ElectrolytePcSaftBinaryRecord(k_ij_2={})", self.k_ij[2]));
            tokens.push(format!("ElectrolytePcSaftBinaryRecord(k_ij_3={})", self.k_ij[3]));
            tokens.push(")".to_string());}
            if let Some(association) = self.association {
                if let Some(kappa_ab) = association.kappa_ab {
                    tokens.push(format!("kappa_ab={}", kappa_ab));
                }
                if let Some(epsilon_k_ab) = association.epsilon_k_ab {
                    tokens.push(format!("epsilon_k_ab={}", epsilon_k_ab));
                }
            }
            write!(f, "PcSaftBinaryRecord({})", tokens.join(", "))
    }
}

pub struct ElectrolytePcSaftParameters {
    pub molarweight: Array1<f64>,
    pub m: Array1<f64>,
    pub sigma: Array1<f64>,
    pub epsilon_k: Array1<f64>,
    pub mu: Array1<f64>,
    pub q: Array1<f64>,
    pub mu2: Array1<f64>,
    pub q2: Array1<f64>,
    pub association: AssociationParameters,
    pub z: Array1<f64>,
    pub k_ij: Array2<Vec<f64>>,
    pub sigma_ij: Array2<f64>,
    pub e_k_ij: Array2<f64>,
    pub ndipole: usize,
    pub nquadpole: usize,
    pub nionic: usize,
    pub nsolvent: usize,
    pub sigma_t_comp: Array1<usize>,
    pub dipole_comp: Array1<usize>,
    pub quadpole_comp: Array1<usize>,
    pub ionic_comp: Array1<usize>,
    pub solvent_comp: Array1<usize>,
    pub viscosity: Option<Array2<f64>>,
    pub diffusion: Option<Array2<f64>>,
    pub permittivity: Option<PermittivityRecord>,
    pub thermal_conductivity: Option<Array2<f64>>,
    pub pure_records: Vec<PureRecord<ElectrolytePcSaftRecord>>,
    pub binary_records: Option<Array2<ElectrolytePcSaftBinaryRecord>>,
}

impl Parameter for ElectrolytePcSaftParameters {
    type Pure = ElectrolytePcSaftRecord;
    type Binary = ElectrolytePcSaftBinaryRecord;

    fn from_records(
        pure_records: Vec<PureRecord<Self::Pure>>,
        binary_records: Option<Array2<Self::Binary>>,
    ) -> Result<Self, ParameterError> {
        let n = pure_records.len();

        let mut molarweight = Array::zeros(n);
        let mut m = Array::zeros(n);
        let mut sigma = Array::zeros(n);
        let mut epsilon_k = Array::zeros(n);
        let mut mu = Array::zeros(n);
        let mut q = Array::zeros(n);
        let mut z = Array::zeros(n);
        let mut association_records = Vec::with_capacity(n);
        let mut viscosity = Vec::with_capacity(n);
        let mut diffusion = Vec::with_capacity(n);
        let mut thermal_conductivity = Vec::with_capacity(n);

        let mut component_index = HashMap::with_capacity(n);

        for (i, record) in pure_records.iter().enumerate() {
            component_index.insert(record.identifier.clone(), i);
            let r = &record.model_record;
            m[i] = r.m;
            sigma[i] = r.sigma;
            epsilon_k[i] = r.epsilon_k;
            mu[i] = r.mu.unwrap_or(0.0);
            q[i] = r.q.unwrap_or(0.0);
            z[i] = r.z.unwrap_or(0.0);
            association_records.push(r.association_record.into_iter().collect());
            viscosity.push(r.viscosity);
            diffusion.push(r.diffusion);
            thermal_conductivity.push(r.thermal_conductivity);
            molarweight[i] = record.molarweight;
        }

        let mu2 = &mu * &mu / (&m * &sigma * &sigma * &sigma * &epsilon_k)
            * 1e-19
            * (JOULE / KELVIN / KB).into_value();
        let q2 = &q * &q / (&m * &sigma.mapv(|s| s.powi(5)) * &epsilon_k)
            * 1e-19
            * (JOULE / KELVIN / KB).into_value();
        let dipole_comp: Array1<usize> = mu2
            .iter()
            .enumerate()
            .filter_map(|(i, &mu2)| (mu2.abs() > 0.0).then_some(i))
            .collect();
        let ndipole = dipole_comp.len();
        let quadpole_comp: Array1<usize> = q2
            .iter()
            .enumerate()
            .filter_map(|(i, &q2)| (q2.abs() > 0.0).then_some(i))
            .collect();
        let nquadpole = quadpole_comp.len();

        let binary_association: Vec<_> = binary_records
            .iter()
            .flat_map(|r| {
                r.indexed_iter()
                    .filter_map(|(i, record)| record.association.map(|r| (i, r)))
            })
            .collect();
        let association =
            AssociationParameters::new(&association_records, &sigma, &binary_association, None);

        let ionic_comp: Array1<usize> = z
            .iter()
            .enumerate()
            .filter_map(|(i, &zi)| (zi.abs() > 0.0).then_some(i))
            .collect();

        let nionic = ionic_comp.len();

        let solvent_comp: Array1<usize> = z
            .iter()
            .enumerate()
            .filter_map(|(i, &zi)| (zi.abs() == 0.0).then_some(i))
            .collect();
        let nsolvent = solvent_comp.len();

        let mut bool_sigma_t = Array1::zeros(n);
        for i in 0..n {
            let name = pure_records[i]
                .identifier
                .name
                .clone()
                .unwrap_or(String::from("unknown"));
            if name.contains("sigma_t") {
                bool_sigma_t[i] = 1usize
            }
        }
        let sigma_t_comp: Array1<usize> = Array::from_iter(
            bool_sigma_t
                .iter()
                .enumerate()
                .filter(|x| x.1 == &1usize)
                .map(|x| x.0),
        );

        let mut k_ij: Array2<Vec<f64>> = Array2::from_elem((n, n), vec![0., 0., 0., 0.]);

        if let Some(binary_records) = binary_records.as_ref() {
            for i in 0..n {
                for j in 0..n {
                    let temp_kij = binary_records[[i, j]].k_ij.clone();
                    if temp_kij.len() > 4 {
                        panic!("Binary interaction for component {} with {} is parametrized with more than 4 k_ij coefficients.", i, j);
                    } else {
                        (0..temp_kij.len()).for_each(|k| {
                            k_ij[[i, j]][k] = temp_kij[k];
                        });
                    }
                }
            }

            // No binary interaction between charged species of same kind (+/+ and -/-)
            ionic_comp.iter().for_each(|ai| {
                k_ij[[*ai, *ai]][0] = 1.0;
                for k in 1..4usize {
                    k_ij[[*ai, *ai]][k] = 0.0;
                }
            });
        }

        let mut sigma_ij = Array::zeros((n, n));
        let mut e_k_ij = Array::zeros((n, n));
        for i in 0..n {
            for j in 0..n {
                e_k_ij[[i, j]] = (epsilon_k[i] * epsilon_k[j]).sqrt();
                sigma_ij[[i, j]] = 0.5 * (sigma[i] + sigma[j]);
            }
        }

        let viscosity_coefficients = if viscosity.iter().any(|v| v.is_none()) {
            None
        } else {
            let mut v = Array2::zeros((4, viscosity.len()));
            for (i, vi) in viscosity.iter().enumerate() {
                v.column_mut(i).assign(&Array1::from(vi.unwrap().to_vec()));
            }
            Some(v)
        };

        let diffusion_coefficients = if diffusion.iter().any(|v| v.is_none()) {
            None
        } else {
            let mut v = Array2::zeros((5, diffusion.len()));
            for (i, vi) in diffusion.iter().enumerate() {
                v.column_mut(i).assign(&Array1::from(vi.unwrap().to_vec()));
            }
            Some(v)
        };

        let thermal_conductivity_coefficients = if thermal_conductivity.iter().any(|v| v.is_none())
        {
            None
        } else {
            let mut v = Array2::zeros((4, thermal_conductivity.len()));
            for (i, vi) in thermal_conductivity.iter().enumerate() {
                v.column_mut(i).assign(&Array1::from(vi.unwrap().to_vec()));
            }
            Some(v)
        };

        // Permittivity
        let permittivity_records: Array1<PermittivityRecord> = pure_records
            .iter()
            .filter(|&record| (record.model_record.permittivity_record.is_some())).map(|record| record.clone().model_record.permittivity_record.unwrap())
            .collect();

        if nionic != 0 && permittivity_records.len() < nsolvent {
            panic!("Provide permittivity records for each solvent.")
        }

        let mut modeltype = -1;
        let mut mu_scaling: Vec<f64> = vec![];
        let mut alpha_scaling: Vec<f64> = vec![];
        let mut ci_param: Vec<f64> = vec![];
        let mut points: Vec<Vec<(f64, f64)>> = vec![];

        permittivity_records
            .iter()
            .enumerate()
            .for_each(|(i, record)| {
                match record {
                    PermittivityRecord::PerturbationTheory {
                        dipole_scaling,
                        polarizability_scaling,
                        correlation_integral_parameter,
                    } => {
                        if modeltype == 2 {
                            panic!("Inconsistent models for permittivity.")
                        };
                        modeltype = 1;
                        mu_scaling.push(dipole_scaling[0]);
                        alpha_scaling.push(polarizability_scaling[0]);
                        ci_param.push(correlation_integral_parameter[0]);
                    }
                    PermittivityRecord::ExperimentalData { data } => {
                        if modeltype == 1 {
                            panic!("Inconsistent models for permittivity.")
                        };
                        modeltype = 2;
                        points.push(data[0].clone());
                        // Check if experimental data points are sorted
                        let mut t_check = 0.0;
                        for point in &data[0] {
                            if point.0 < t_check {
                                panic!("Permittivity points for component {} are unsorted.", i);
                            }
                            t_check = point.0;
                        }
                    }
                }
            });

        let permittivity = match modeltype {
            1 => Some(PermittivityRecord::PerturbationTheory {
                dipole_scaling: mu_scaling,
                polarizability_scaling: alpha_scaling,
                correlation_integral_parameter: ci_param,
            }),
            2 => Some(PermittivityRecord::ExperimentalData { data: points }),
            _ => None,
        };

        if nionic > 0 && permittivity.is_none() {
            panic!("Permittivity of one or more solvents must be specified.")
        };

        Ok(Self {
            molarweight,
            m,
            sigma,
            epsilon_k,
            mu,
            q,
            mu2,
            q2,
            association,
            z,
            k_ij,
            sigma_ij,
            e_k_ij,
            ndipole,
            nquadpole,
            nionic,
            nsolvent,
            dipole_comp,
            quadpole_comp,
            ionic_comp,
            solvent_comp,
            sigma_t_comp,
            viscosity: viscosity_coefficients,
            diffusion: diffusion_coefficients,
            thermal_conductivity: thermal_conductivity_coefficients,
            permittivity,
            pure_records,
            binary_records
        })
    }

    fn records(
        &self,
    ) -> (
        &[PureRecord<ElectrolytePcSaftRecord>],
        Option<&Array2<ElectrolytePcSaftBinaryRecord>>,
    ) {
        (&self.pure_records, self.binary_records.as_ref())
    }
    
}

impl HardSphereProperties for ElectrolytePcSaftParameters {
    fn monomer_shape<N: DualNum<f64>>(&self, _: N) -> MonomerShape<N> {
        MonomerShape::NonSpherical(self.m.mapv(N::from))
    }

    fn hs_diameter<D: DualNum<f64>>(&self, temperature: D) -> Array1<D> {
        let sigma_t = self.sigma_t(temperature.clone());

        let ti = temperature.recip() * -3.0;
        let mut d = Array::from_shape_fn(sigma_t.len(), |i| {
            -((ti.clone() * self.epsilon_k[i]).exp() * 0.12 - 1.0) * sigma_t[i]
        });
        for i in 0..self.nionic {
            let ai = self.ionic_comp[i];
            d[ai] = D::one() * sigma_t[ai] * (1.0 - 0.12);
        }
        d
    }

    fn sigma_t<D: DualNum<f64>>(&self, temperature: D) -> Array1<f64> {
        let mut sigma_t: Array1<f64> = Array::from_shape_fn(self.sigma.len(), |i| self.sigma[i]);
        for i in 0..self.sigma_t_comp.len() {
            sigma_t[i] = (sigma_t[i] + (temperature.re() * -0.01775).exp() * 10.11
                - (temperature.re() * -0.01146).exp() * 1.417)
                .re()
        }
        sigma_t
    }

    fn sigma_ij_t<D: DualNum<f64>>(&self, temperature: D) -> Array2<f64> {
        
        let diameter = self.sigma_t(temperature);
        let n = diameter.len();

        let mut sigma_ij_t = Array::zeros((n, n));
        for i in 0..n {
            for j in 0..n {
                sigma_ij_t[[i, j]] = (diameter[i] + diameter[j]) * 0.5;
            }
        }
        sigma_ij_t
    }
}

impl ElectrolytePcSaftParameters {
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();
        let o = &mut output;
        write!(
            o,
            "|component|molarweight|$m$|$\\sigma$|$\\varepsilon$|$\\mu$|$Q$|$z$|$\\kappa_{{AB}}$|$\\varepsilon_{{AB}}$|$N_A$|$N_B$|\n|-|-|-|-|-|-|-|-|-|-|-|-|"
        )
        .unwrap();
        for (i, record) in self.pure_records.iter().enumerate() {
            let component = record.identifier.name.clone();
            let component = component.unwrap_or(format!("Component {}", i + 1));
            let association = record
                .model_record
                .association_record
                .unwrap_or_else(|| AssociationRecord::new(0.0, 0.0, 0.0, 0.0, 0.0));
            write!(
                o,
                "\n|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|",
                component,
                record.molarweight,
                record.model_record.m,
                record.model_record.sigma,
                record.model_record.epsilon_k,
                record.model_record.mu.unwrap_or(0.0),
                record.model_record.q.unwrap_or(0.0),
                record.model_record.z.unwrap_or(0.0),
                association.kappa_ab,
                association.epsilon_k_ab,
                association.na,
                association.nb,
                association.nc
            )
            .unwrap();
        }

        output
    }
}


#[allow(dead_code)]
#[cfg(test)]
pub mod utils {
    use super::*;
    use std::sync::Arc;

    pub fn propane_parameters() -> Arc<ElectrolytePcSaftParameters> {
        let propane_json = r#"
            {
                "identifier": {
                    "cas": "74-98-6",
                    "name": "propane",
                    "iupac_name": "propane",
                    "smiles": "CCC",
                    "inchi": "InChI=1/C3H8/c1-3-2/h3H2,1-2H3",
                    "formula": "C3H8"
                },
                "model_record": {
                    "m": 2.001829,
                    "sigma": 3.618353,
                    "epsilon_k": 208.1101,
                    "viscosity": [-0.8013, -1.9972,-0.2907, -0.0467],
                    "thermal_conductivity": [-0.15348,  -0.6388, 1.21342, -0.01664],
                    "diffusion": [-0.675163251512047, 0.3212017677695878, 0.100175249144429, 0.0, 0.0]
                },
                "molarweight": 44.0962
            }"#;
        let propane_record: PureRecord<ElectrolytePcSaftRecord> =
            serde_json::from_str(propane_json).expect("Unable to parse json.");
        Arc::new(ElectrolytePcSaftParameters::new_pure(propane_record).unwrap())
    }

    pub fn carbon_dioxide_parameters() -> ElectrolytePcSaftParameters {
        let co2_json = r#"
        {
            "identifier": {
                "cas": "124-38-9",
                "name": "carbon-dioxide",
                "iupac_name": "carbon dioxide",
                "smiles": "O=C=O",
                "inchi": "InChI=1/CO2/c2-1-3",
                "formula": "CO2"
            },
            "molarweight": 44.0098,
            "model_record": {
                "m": 1.5131,
                "sigma": 3.1869,
                "epsilon_k": 163.333,
                "q": 4.4
            }
        }"#;
        let co2_record: PureRecord<ElectrolytePcSaftRecord> =
            serde_json::from_str(co2_json).expect("Unable to parse json.");
        ElectrolytePcSaftParameters::new_pure(co2_record).unwrap()
    }

    pub fn butane_parameters() -> Arc<ElectrolytePcSaftParameters> {
        let butane_json = r#"
            {
                "identifier": {
                    "cas": "106-97-8",
                    "name": "butane",
                    "iupac_name": "butane",
                    "smiles": "CCCC",
                    "inchi": "InChI=1/C4H10/c1-3-4-2/h3-4H2,1-2H3",
                    "formula": "C4H10"
                },
                "model_record": {
                    "m": 2.331586,
                    "sigma": 3.7086010000000003,
                    "epsilon_k": 222.8774
                },
                "molarweight": 58.123
            }"#;
        let butane_record: PureRecord<ElectrolytePcSaftRecord> =
            serde_json::from_str(butane_json).expect("Unable to parse json.");
        Arc::new(ElectrolytePcSaftParameters::new_pure(butane_record).unwrap())
    }

    pub fn dme_parameters() -> ElectrolytePcSaftParameters {
        let dme_json = r#"
            {
                "identifier": {
                    "cas": "115-10-6",
                    "name": "dimethyl-ether",
                    "iupac_name": "methoxymethane",
                    "smiles": "COC",
                    "inchi": "InChI=1/C2H6O/c1-3-2/h1-2H3",
                    "formula": "C2H6O"
                },
                "model_record": {
                    "m": 2.2634,
                    "sigma": 3.2723,
                    "epsilon_k": 210.29,
                    "mu": 1.3
                },
                "molarweight": 46.0688
            }"#;
        let dme_record: PureRecord<ElectrolytePcSaftRecord> =
            serde_json::from_str(dme_json).expect("Unable to parse json.");
        ElectrolytePcSaftParameters::new_pure(dme_record).unwrap()
    }

    pub fn water_parameters_sigma_t() -> ElectrolytePcSaftParameters {
        let water_json = r#"
        {
                "identifier": {
                    "cas": "7732-18-5",
                    "name": "water_np_sigma_t",
                    "iupac_name": "oxidane",
                    "smiles": "O",
                    "inchi": "InChI=1/H2O/h1H2",
                    "formula": "H2O"
                },
                "model_record": {
                    "m": 1.2047,
                    "sigma": 2.7927,
                    "epsilon_k": 353.95,
                    "kappa_ab": 0.04509,
                    "epsilon_k_ab": 2425.7
                },
                "molarweight": 18.0152
              }"#;
        let water_record: PureRecord<ElectrolytePcSaftRecord> =
            serde_json::from_str(water_json).expect("Unable to parse json.");
        ElectrolytePcSaftParameters::new_pure(water_record).unwrap()
    }

    pub fn water_nacl_parameters() -> ElectrolytePcSaftParameters {
        // Water parameters from Held et al. (2014), originally from Fuchs et al. (2006)
        let pure_json = r#"[
            {
                "identifier": {
                    "cas": "7732-18-5",
                    "name": "water_np_sigma_t",
                    "iupac_name": "oxidane",
                    "smiles": "O",
                    "inchi": "InChI=1/H2O/h1H2",
                    "formula": "H2O"
                },
                "saft_record": {
                    "m": 1.2047,
                    "sigma": 2.7927,
                    "epsilon_k": 353.95,
                    "kappa_ab": 0.04509,
                    "epsilon_k_ab": 2425.7
                },
                "molarweight": 18.0152
            },
            {
                "identifier": {
                    "cas": "110-54-3",
                    "name": "na+",
                    "formula": "na+"
                },
                "saft_record": {
                    "m": 1,
                    "sigma": 2.8232,
                    "epsilon_k": 230.0,
                    "z": 1
                },
                "molarweight": 22.98976
            },
            {
                "identifier": {
                    "cas": "7782-50-5",
                    "name": "cl-",
                    "formula": "cl-"
                },
                "saft_record": {
                    "m": 1,
                    "sigma": 2.7560,
                    "epsilon_k": 170,
                    "z": -1
                },
                "molarweight": 35.45
            }
            ]"#;
        let binary_json = r#"[
                {
                    "id1": {
                        "cas": "7732-18-5",
                        "name": "water_np",
                        "iupac_name": "oxidane",
                        "smiles": "O",
                        "inchi": "InChI=1/H2O/h1H2",
                        "formula": "H2O"
                    },
                    "id2": {
                        "cas": "110-54-3",
                        "name": "na+",
                        "formula": "na+"
                    },
                    "k_ij": [0.0045]
                },
                {
                    "id1": {
                        "cas": "7732-18-5",
                        "name": "water_np",
                        "iupac_name": "oxidane",
                        "smiles": "O",
                        "inchi": "InChI=1/H2O/h1H2",
                        "formula": "H2O"
                    },
                    "id2": {
                        "cas": "7782-50-5",
                        "name": "cl-",
                        "formula": "cl-"
                    },
                    "k_ij": [-0.25]
                },
                {
                    "id1": {
                        "cas": "110-54-3",
                        "name": "na+",
                        "formula": "na+"
                    },
                    "id2": {
                        "cas": "7782-50-5",
                        "name": "cl-",
                        "formula": "cl-"
                    },
                    "k_ij": [0.317]
                }
                ]"#;
        let pure_records: Vec<PureRecord<ElectrolytePcSaftRecord>> =
            serde_json::from_str(pure_json).expect("Unable to parse json.");
        let binary_records: ElectrolytePcSaftBinaryRecord =
            serde_json::from_str(binary_json).expect("Unable to parse json.");
        ElectrolytePcSaftParameters::new_binary(pure_records, Some(binary_records)).unwrap()
    }

    pub fn water_parameters() -> ElectrolytePcSaftParameters {
        let water_json = r#"
            {
                "identifier": {
                    "cas": "7732-18-5",
                    "name": "water_np",
                    "iupac_name": "oxidane",
                    "smiles": "O",
                    "inchi": "InChI=1/H2O/h1H2",
                    "formula": "H2O"
                },
                "model_record": {
                    "m": 1.065587,
                    "sigma": 3.000683,
                    "epsilon_k": 366.5121,
                    "kappa_ab": 0.034867983,
                    "epsilon_k_ab": 2500.6706,
                    "na": 1.0,
                    "nb": 1.0
                },
                "molarweight": 18.0152
            }"#;
        let water_record: PureRecord<ElectrolytePcSaftRecord> =
            serde_json::from_str(water_json).expect("Unable to parse json.");
        ElectrolytePcSaftParameters::new_pure(water_record).unwrap()
    }

    pub fn dme_co2_parameters() -> ElectrolytePcSaftParameters {
        let binary_json = r#"[
            {
                "identifier": {
                    "cas": "115-10-6",
                    "name": "dimethyl-ether",
                    "iupac_name": "methoxymethane",
                    "smiles": "COC",
                    "inchi": "InChI=1/C2H6O/c1-3-2/h1-2H3",
                    "formula": "C2H6O"
                },
                "molarweight": 46.0688,
                "model_record": {
                    "m": 2.2634,
                    "sigma": 3.2723,
                    "epsilon_k": 210.29,
                    "mu": 1.3
                }
            },
            {
                "identifier": {
                    "cas": "124-38-9",
                    "name": "carbon-dioxide",
                    "iupac_name": "carbon dioxide",
                    "smiles": "O=C=O",
                    "inchi": "InChI=1/CO2/c2-1-3",
                    "formula": "CO2"
                },
                "molarweight": 44.0098,
                "model_record": {
                    "m": 1.5131,
                    "sigma": 3.1869,
                    "epsilon_k": 163.333,
                    "q": 4.4
                }
            }
        ]"#;
        let binary_record: Vec<PureRecord<ElectrolytePcSaftRecord>> =
            serde_json::from_str(binary_json).expect("Unable to parse json.");
        ElectrolytePcSaftParameters::new_binary(binary_record, None).unwrap()
    }

    pub fn propane_butane_parameters() -> Arc<ElectrolytePcSaftParameters> {
        let binary_json = r#"[
            {
                "identifier": {
                    "cas": "74-98-6",
                    "name": "propane",
                    "iupac_name": "propane",
                    "smiles": "CCC",
                    "inchi": "InChI=1/C3H8/c1-3-2/h3H2,1-2H3",
                    "formula": "C3H8"
                },
                "model_record": {
                    "m": 2.0018290000000003,
                    "sigma": 3.618353,
                    "epsilon_k": 208.1101,
                    "viscosity": [-0.8013, -1.9972, -0.2907, -0.0467],
                    "thermal_conductivity": [-0.15348, -0.6388, 1.21342, -0.01664],
                    "diffusion": [-0.675163251512047, 0.3212017677695878, 0.100175249144429, 0.0, 0.0]
                },
                "molarweight": 44.0962
            },
            {
                "identifier": {
                    "cas": "106-97-8",
                    "name": "butane",
                    "iupac_name": "butane",
                    "smiles": "CCCC",
                    "inchi": "InChI=1/C4H10/c1-3-4-2/h3-4H2,1-2H3",
                    "formula": "C4H10"
                },
                "model_record": {
                    "m": 2.331586,
                    "sigma": 3.7086010000000003,
                    "epsilon_k": 222.8774,
                    "viscosity": [-0.9763, -2.2413, -0.3690, -0.0605],
                    "diffusion": [-0.8985872992958458, 0.3428584416613513, 0.10236616087103916, 0.0, 0.0]
                },
                "molarweight": 58.123
            }
        ]"#;
        let binary_record: Vec<PureRecord<ElectrolytePcSaftRecord>> =
            serde_json::from_str(binary_json).expect("Unable to parse json.");
        Arc::new(ElectrolytePcSaftParameters::new_binary(binary_record, None).unwrap())
    }
}