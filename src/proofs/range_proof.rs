/*

Copyright 2018 by Kzen Networks

This file is part of bulletproof library
(https://github.com/KZen-networks/bulletproof)

bulletproof is free software: you can redistribute
it and/or modify it under the terms of the GNU General Public
License as published by the Free Software Foundation, either
version 3 of the License, or (at your option) any later version.

@license GPL-3.0+ <https://github.com/KZen-networks/bulletproof/blob/master/LICENSE>
*/

// based on the paper: https://eprint.iacr.org/2017/1066.pdf
use cryptography_utils::arithmetic::traits::{Converter, Modulo};
use cryptography_utils::cryptographic_primitives::hashing::hash_sha256::HSha256;
use cryptography_utils::cryptographic_primitives::hashing::hash_sha512::HSha512;
use cryptography_utils::cryptographic_primitives::hashing::traits::*;
use cryptography_utils::elliptic::curves::traits::*;
use cryptography_utils::BigInt;
use cryptography_utils::{FE, GE};
use itertools::Itertools;
use proofs::inner_product::InnerProductArg;
use std::ops::{Shl, Shr};
use Errors::{self, RangeProofError};
pub struct RangeProof {
    A: GE,
    S: GE,
    T1: GE,
    T2: GE,
    tau_x: FE,
    miu: FE,
    tx: FE,
    inner_product_proof: InnerProductArg,
}

impl RangeProof {
    pub fn prove(
        g_vec: &Vec<GE>,
        h_vec: &Vec<GE>,
        G: &GE,
        H: &GE,
        mut secret: Vec<FE>,
        mut blinding: Vec<FE>,
        bit_length: usize,
    ) -> RangeProof {
        let num_of_proofs = secret.len();
        //num of proofs times bit length
        let nm = num_of_proofs * bit_length;
        let alpha: FE = ECScalar::new_random();
        let rho: FE = ECScalar::new_random();

        let g_vec = g_vec.to_vec();
        let h_vec = h_vec.to_vec();

        let mut A = H.clone() * &alpha;
        let mut S = H.clone() * &rho;
        let two = BigInt::from(2);
        let one = BigInt::from(1);
        let order = alpha.q();

        //concat all secrets:
        secret.reverse();
        let secret_agg = secret.iter().fold(BigInt::zero(), |acc, x| {
            acc.shl(bit_length) + x.to_big_int()
        });

        let aL = (0..nm)
            .map(|i| {
                let shr_secret = secret_agg.clone().shr(i);
                shr_secret.modulus(&two)
            }).collect::<Vec<BigInt>>();
        let aR = (0..nm)
            .map(|i| BigInt::mod_sub(&aL[i], &one, &order))
            .collect::<Vec<BigInt>>();

        let secret_bits = (0..nm)
            .map(|i| {
                let bignum_bit: BigInt = aL[i].clone() & BigInt::one();
                let byte = BigInt::to_vec(&bignum_bit);
                byte[0] == 1
            }).collect::<Vec<bool>>();
        let mut index: usize = 0;
        A = g_vec.iter().zip(secret_bits.clone()).fold(A, |acc, x| {
            if x.1 {
                acc.add_point(&x.0.get_element())
            } else {
                acc
            }
        });

        A = h_vec.iter().zip(secret_bits.clone()).fold(A, |acc, x| {
            if !x.1 {
                acc.sub_point(&x.0.get_element())
            } else {
                acc
            }
        });

        let SR = (0..nm).map(|_| ECScalar::new_random()).collect::<Vec<FE>>();
        let SL = (0..nm).map(|_| ECScalar::new_random()).collect::<Vec<FE>>();

        S = SL.iter().zip(&SR).fold(S, |acc, x| {
            let g_vec_i: GE = g_vec[index].clone();
            let h_vec_i: GE = h_vec[index].clone();
            index = index + 1;
            let g_vec_i_SLi = g_vec_i * x.0;
            let h_vec_i_SRi = h_vec_i * x.1;
            let SRhi_plus_SLgi = h_vec_i_SRi + g_vec_i_SLi;
            acc + SRhi_plus_SLgi
        });

        let y = HSha256::create_hash_from_ge(&[&A, &S]);
        let base_point: GE = ECPoint::generator();
        let yG: GE = base_point * &y;
        let z = HSha256::create_hash_from_ge(&[&yG]);
        let z_bn = z.to_big_int();
        let z_squared = BigInt::mod_pow(&z.to_big_int(), &BigInt::from(2), &order);
        let z_cubed = BigInt::mod_pow(&z.to_big_int(), &BigInt::from(3), &order);

        let yi = (0..nm)
            .map(|i| BigInt::mod_pow(&y.to_big_int(), &BigInt::from(i as u32), &order))
            .collect::<Vec<BigInt>>();

        let t2 = (0..nm)
            .map(|i| {
                let t2_1 = BigInt::mod_mul(&SR[i].to_big_int(), &yi[i], &order);
                let t2_2 = BigInt::mod_mul(&t2_1, &SL[i].to_big_int(), &order);
                t2_2
            }).fold(BigInt::zero(), |acc, x| BigInt::mod_add(&acc, &x, &order));

        let vec_2n = (0..bit_length)
            .map(|i| BigInt::mod_pow(&two, &BigInt::from(i as u32), &order))
            .collect::<Vec<BigInt>>();

        let t0 = (0..nm)
            .map(|i| {
                let t0_1 = BigInt::mod_sub(&aL[i], &z_bn, &order);
                let t0_2 = BigInt::mod_add(&aR[i], &z_bn, &order);
                let t0_3 = BigInt::mod_mul(&yi[i], &t0_2, &order);
                let j = (i / bit_length) + 2;
                let k = i % bit_length;
                let z_index = BigInt::mod_pow(&z_bn, &BigInt::from(j as u32), &order);
                let two_to_the_i = vec_2n[k].clone();
                let t0_4 = BigInt::mod_mul(&z_index, &two_to_the_i, &order);
                let t0_5 = BigInt::mod_add(&t0_4, &t0_3, &order);
                BigInt::mod_mul(&t0_5, &t0_1, &order)
            }).fold(BigInt::zero(), |acc, x| BigInt::mod_add(&acc, &x, &order));

        let t1 = (0..nm)
            .map(|i| {
                let t1_1 = BigInt::mod_add(&aR[i], &z_bn, &order);
                let t1_2 = BigInt::mod_mul(&t1_1, &yi[i], &order);
                let t1_3 = BigInt::mod_mul(&SL[i].to_big_int(), &t1_2, &order);
                let t1_4 = BigInt::mod_sub(&aL[i], &z_bn, &order);
                let t1_5 = BigInt::mod_mul(&SR[i].to_big_int(), &yi[i], &order);
                let t1_6 = BigInt::mod_mul(&t1_4, &t1_5, &order);
                let j = i / bit_length + 2;
                let k = i % bit_length;
                let z_index = BigInt::mod_pow(&z_bn, &BigInt::from(j as u32), &order);
                let two_to_the_i = vec_2n[k].clone();
                let t1_7 = BigInt::mod_mul(&z_index, &two_to_the_i, &order);
                let t1_8 = BigInt::mod_mul(&t1_7, &SL[i].to_big_int(), &order);
                let t1_68 = BigInt::mod_add(&t1_6, &t1_8, &order);
                BigInt::mod_add(&t1_3, &t1_68, &order)
            }).fold(BigInt::zero(), |acc, x| BigInt::mod_add(&acc, &x, &order));

        let tau1: FE = ECScalar::new_random();
        let tau2: FE = ECScalar::new_random();
        let t1_fe = ECScalar::from(&t1);
        let t2_fe = ECScalar::from(&t2);
        let T1 = (G.clone() * &t1_fe) + (H.clone() * &tau1);
        let T2 = (G.clone() * &t2_fe) + (H.clone() * &tau2);

        let fs_challenge = HSha256::create_hash_from_ge(&[&T1, &T2, G, H]);
        let fs_challenge_square = fs_challenge.mul(&fs_challenge.get_element());
        let taux_1 = fs_challenge.mul(&tau1.get_element());
        let taux_2 = fs_challenge_square.mul(&tau2.get_element());
        let z_squared_fe: FE = ECScalar::from(&z_squared);

        let scalar_mul_2n = vec_2n.iter().fold(BigInt::zero(), |acc, x: &BigInt| {
            BigInt::mod_add(&acc, x, &order)
        });
        let taux_3 = (0..num_of_proofs)
            .map(|i| {
                let j = BigInt::mod_add(&two, &BigInt::from(i as u32), &order);
                let z_j = BigInt::mod_pow(&z_bn, &j, &order);
                let z_j_fe: FE = ECScalar::from(&z_j);
                z_j_fe.mul(&blinding[i].get_element())
            }).fold(taux_2, |acc, x| acc.add(&x.get_element()));
        let tau_x = taux_1.add(&taux_3.get_element());
        let miu = (rho.mul(&fs_challenge.get_element())).add(&alpha.get_element());

        let Lp = (0..nm)
            .map(|i| {
                let Lp_1 = (SL[i].mul(&fs_challenge.get_element())).to_big_int();
                let Lp_2 = BigInt::mod_sub(&aL[i], &z_bn, &order);
                BigInt::mod_add(&Lp_1, &Lp_2, &order)
            }).collect::<Vec<BigInt>>();

        let Rp = (0..nm)
            .map(|i| {
                let Rp_1 = (SR[i].mul(&fs_challenge.get_element())).to_big_int();

                let j = i / bit_length + 2;
                let k = i % bit_length;
                let z_index = BigInt::mod_pow(&z_bn, &BigInt::from(j as u32), &order);
                let two_to_the_i = vec_2n[k].clone();
                let Rp_2 = BigInt::mod_mul(&z_index, &two_to_the_i, &order);
                let Rp_3 = BigInt::mod_add(&BigInt::mod_add(&z_bn, &aR[i], &order), &Rp_1, &order);
                let Rp_4 = BigInt::mod_mul(&yi[i], &Rp_3, &order);
                BigInt::mod_add(&Rp_4, &Rp_2, &order)
            }).collect::<Vec<BigInt>>();
        let tx = Lp.iter().zip(&Rp).fold(BigInt::zero(), |acc, x| {
            let Lp_iRp_i = BigInt::mod_mul(x.0, x.1, &order);
            BigInt::mod_add(&acc, &Lp_iRp_i, &order)
        });
        let tx_fe: FE = ECScalar::from(&tx);
        let challenge_x = HSha256::create_hash(&[&tau_x.to_big_int(), &miu.to_big_int(), &tx]);
        let challenge_x: FE = ECScalar::from(&challenge_x);
        let Gx = G.clone() * &challenge_x;
        // P' = u^{xc}
        let P = Gx.clone() * &tx_fe;

        let yi_inv = (0..nm)
            .map(|i| {
                let yi_fe: FE = ECScalar::from(&yi[i]);
                yi_fe.invert()
            }).collect::<Vec<FE>>();

        let hi_tag = (0..nm)
            .map(|i| h_vec[i].clone() * &yi_inv[i])
            .collect::<Vec<GE>>();

        // P' = P' g^l
        let P = g_vec.iter().zip(&Lp).fold(P, |acc, x| {
            let g_vec_i_lp_i = x.0.clone() * &ECScalar::from(x.1);
            acc + g_vec_i_lp_i
        });
        // P' = P' h'^r
        let P = hi_tag.iter().zip(&Rp).fold(P, |acc, x| {
            let h_vec_i_rp_i = x.0.clone() * &ECScalar::from(x.1);
            acc + h_vec_i_rp_i
        });
        // line 9
        // public input : g,h,u^x,P' = g_vec, hi_tag, Gx,P
        // private input: a,b  = Lp,Rp
        let mut L_vec = Vec::with_capacity(nm);
        let mut R_vec = Vec::with_capacity(nm);
        let inner_product_proof =
            InnerProductArg::prove(g_vec, hi_tag, Gx, P, Lp, Rp, L_vec, R_vec);

        let sec = secret.clone();
        let mut vec_ped_zm = (0..num_of_proofs)
            .map(|i| {
                let z_2_m = BigInt::mod_pow(&z_bn, &BigInt::from((2 + i) as u32), &order);
                let z_2_m_fe: FE = ECScalar::from(&z_2_m);
                sec[i].mul(&z_2_m_fe.get_element())
            }).collect::<Vec<FE>>();
        let vec_ped_zm_1 = vec_ped_zm.remove(0);
        let ped_com_sum = vec_ped_zm
            .iter()
            .fold(vec_ped_zm_1, |acc, x| acc.add(&x.get_element()));

        // delta(x,y):
        let yi = (0..nm)
            .map(|i| BigInt::mod_pow(&y.to_big_int(), &BigInt::from(i as u32), &order))
            .collect::<Vec<BigInt>>();

        let scalar_mul_yn = yi
            .iter()
            .fold(BigInt::zero(), |acc, x| BigInt::mod_add(&acc, x, &order));

        let two = BigInt::from(2);
        let vec_2n = (0..bit_length)
            .map(|i| BigInt::mod_pow(&two, &BigInt::from(i as u32), &order))
            .collect::<Vec<BigInt>>();

        let scalar_mul_2n = vec_2n.iter().fold(BigInt::zero(), |acc, x: &BigInt| {
            BigInt::mod_add(&acc, x, &order)
        });

        let z_cubed_scalar_mul_2n = (0..num_of_proofs)
            .map(|i| {
                let j = BigInt::mod_add(&BigInt::from(3), &BigInt::from(i as u32), &order);
                let z_j = BigInt::mod_pow(&z_bn, &j, &order);
                BigInt::mod_mul(&z_j, &scalar_mul_2n, &order)
            }).fold(BigInt::zero(), |acc, x| BigInt::mod_add(&acc, &x, &order));

        let z_minus_zsq = BigInt::mod_sub(&z_bn, &z_squared, &order);
        let z_minus_zsq_scalar_mul_yn = BigInt::mod_mul(&z_minus_zsq, &scalar_mul_yn, &order);
        let delta = BigInt::mod_sub(&z_minus_zsq_scalar_mul_yn, &z_cubed_scalar_mul_2n, &order);
        let test = BigInt::mod_add(&delta, &ped_com_sum.to_big_int(), &order);
        return RangeProof {
            A,
            S,
            T1,
            T2,
            tau_x,
            miu,
            tx: tx_fe,
            inner_product_proof,
        };
    }

    pub fn verify(
        &self,
        g_vec: &Vec<GE>,
        h_vec: &Vec<GE>,
        G: &GE,
        H: &GE,
        ped_com: Vec<GE>,
        bit_length: usize,
    ) -> Result<bool, Errors> {
        let num_of_proofs = ped_com.len();
        let nm = num_of_proofs * bit_length;
        let g_vec = g_vec.to_vec();
        let h_vec = h_vec.to_vec();

        let y = HSha256::create_hash_from_ge(&[&self.A, &self.S]);
        let base_point: GE = ECPoint::generator();
        let yG: GE = base_point * &y;
        let z = HSha256::create_hash_from_ge(&[&yG]);
        let z_bn = z.to_big_int();
        let order = self.tau_x.q();
        let z_minus = BigInt::mod_sub(&order, &z.to_big_int(), &order);
        let z_minus_fe: FE = ECScalar::from(&z_minus);
        let z_squared = BigInt::mod_pow(&z.to_big_int(), &BigInt::from(2), &order);
        let z_cubed = BigInt::mod_pow(&z.to_big_int(), &BigInt::from(3), &order);
        let z_sq_fe: FE = ECScalar::from(&z_squared);
        // delta(x,y):
        let yi = (0..nm)
            .map(|i| BigInt::mod_pow(&y.to_big_int(), &BigInt::from(i as u32), &order))
            .collect::<Vec<BigInt>>();

        let scalar_mul_yn = yi
            .iter()
            .fold(BigInt::zero(), |acc, x| BigInt::mod_add(&acc, x, &order));

        let two = BigInt::from(2);
        let vec_2n = (0..bit_length)
            .map(|i| BigInt::mod_pow(&two, &BigInt::from(i as u32), &order))
            .collect::<Vec<BigInt>>();

        let scalar_mul_2n = vec_2n.iter().fold(BigInt::zero(), |acc, x: &BigInt| {
            BigInt::mod_add(&acc, x, &order)
        });

        let z_cubed_scalar_mul_2n = (0..num_of_proofs)
            .map(|i| {
                let j = BigInt::mod_add(&BigInt::from(3), &BigInt::from(i as u32), &order);
                let z_j = BigInt::mod_pow(&z_bn, &j, &order);
                BigInt::mod_mul(&z_j, &scalar_mul_2n, &order)
            }).fold(BigInt::zero(), |acc, x| BigInt::mod_add(&acc, &x, &order));

        let z_minus_zsq = BigInt::mod_sub(&z_bn, &z_squared, &order);
        let z_minus_zsq_scalar_mul_yn = BigInt::mod_mul(&z_minus_zsq, &scalar_mul_yn, &order);
        let delta = BigInt::mod_sub(&z_minus_zsq_scalar_mul_yn, &z_cubed_scalar_mul_2n, &order);

        let yi_inv = (0..nm)
            .map(|i| {
                let yi_fe: FE = ECScalar::from(&yi[i]);
                yi_fe.invert()
            }).collect::<Vec<FE>>();

        let hi_tag = (0..nm)
            .map(|i| h_vec[i].clone() * &yi_inv[i])
            .collect::<Vec<GE>>();

        let fs_challenge = HSha256::create_hash_from_ge(&[&self.T1, &self.T2, G, H]);
        let fs_challenge_square = fs_challenge.mul(&fs_challenge.get_element());

        // eq 65:
        let Gtx = G.clone() * &self.tx;
        let Htaux = H.clone() * &self.tau_x;
        let left_side = Gtx + Htaux;
        let delta_fe: FE = ECScalar::from(&delta);
        let Gdelta = G.clone() * &delta_fe;
        let Tx = self.T1.clone() * &fs_challenge;
        let Tx_sq = self.T2.clone() * &fs_challenge_square;

        let mut vec_ped_zm = (0..num_of_proofs)
            .map(|i| {
                let z_2_m = BigInt::mod_pow(&z_bn, &BigInt::from((2 + i) as u32), &order);
                let z_2_m_fe: FE = ECScalar::from(&z_2_m);
                ped_com[i].clone() * z_2_m_fe
            }).collect::<Vec<GE>>();
        let vec_ped_zm_1 = vec_ped_zm.remove(0);
        let ped_com_sum = vec_ped_zm.iter().fold(vec_ped_zm_1, |acc, x| acc + x);
        let right_side = ped_com_sum + Gdelta + Tx + Tx_sq;

        let challenge_x = HSha256::create_hash(&[
            &self.tau_x.to_big_int(),
            &self.miu.to_big_int(),
            &self.tx.to_big_int(),
        ]);
        let challenge_x: FE = ECScalar::from(&challenge_x);
        let Gx = G.clone() * &challenge_x;
        // P' = u^{xc}

        let P = Gx.clone() * &self.tx;
        let minus_miu = BigInt::mod_sub(&self.miu.q(), &self.miu.to_big_int(), &self.miu.q());
        let minus_miu_fe: FE = ECScalar::from(&minus_miu);
        let Hmiu = H.clone() * minus_miu_fe;
        let Sx = self.S.clone() * &fs_challenge;
        let P = Hmiu + P + self.A.clone() + Sx;

        let P1 = (0..nm)
            .map(|i| {
                let z_yn = BigInt::mod_mul(&z_bn, &yi[i], &order);
                let j = i / bit_length;
                let k = i % bit_length;
                let z_j = BigInt::mod_pow(&z_bn, &BigInt::from((2 + j) as u32), &order);
                let z_j_2_n = BigInt::mod_mul(&z_j, &vec_2n[k], &order);
                // let z_sq_2n = BigInt::mod_mul(&z_squared, &vec_2n[i], &order);
                let zyn_zsq2n = BigInt::mod_add(&z_yn, &z_j_2_n, &order);
                let zyn_zsq2n_fe: FE = ECScalar::from(&zyn_zsq2n);
                hi_tag[i].clone() * zyn_zsq2n_fe
            }).fold(P, |acc, x| acc + x);

        let P = (0..nm)
            .map(|i| g_vec[i].clone() * &z_minus_fe)
            .fold(P1, |acc, x| acc + x);
        let verify = self.inner_product_proof.verify(g_vec, hi_tag, Gx, P);
        if verify.is_ok() && left_side.get_element() == right_side.get_element() {
            Ok(true)
        } else {
            Err(RangeProofError)
        }
    }
}

pub fn generate_random_point(bytes: &[u8]) -> GE {
    let result: Result<GE, _> = ECPoint::from_bytes(&bytes);
    if result.is_ok() {
        return result.unwrap();
    } else {
        let two = BigInt::from(2);
        let temp: FE = ECScalar::new_random();
        let bn = BigInt::from(bytes);
        let bn_times_two = BigInt::mod_mul(&bn, &two, &temp.q());
        let bytes = BigInt::to_vec(&bn_times_two);
        return generate_random_point(&bytes);
    }
}

#[cfg(test)]
mod tests {
    use proofs::range_proof::generate_random_point;
    use proofs::range_proof::RangeProof;
    use proofs::inner_product::InnerProductArg;
    use Errors::{self, RangeProofError};
    use cryptography_utils::arithmetic::traits::{Converter, Modulo, Samplable};
    use cryptography_utils::cryptographic_primitives::hashing::hash_sha512::HSha512;
    use cryptography_utils::cryptographic_primitives::hashing::traits::*;
    use cryptography_utils::elliptic::curves::traits::*;
    use cryptography_utils::BigInt;
    use cryptography_utils::{FE, GE};
  //  use itertools::Itertools;



    #[test]
    pub fn test_batch_4_range_proof_32() {
        let n = 32;
        // num of proofs
        let m = 4;
        let nm = n * m;
        let KZen: &[u8] = &[75, 90, 101, 110];
        let kzen_label = BigInt::from(KZen);

        let G: GE = ECPoint::generator();
        let label = BigInt::from(1);
        let hash = HSha512::create_hash(&[&label]);
        let H = generate_random_point(&Converter::to_vec(&hash));

        let g_vec = (0..nm)
            .map(|i| {
                let kzen_label_i = BigInt::from(i as u32) + &kzen_label;
                let hash_i = HSha512::create_hash(&[&kzen_label_i]);
                generate_random_point(&Converter::to_vec(&hash_i))
            }).collect::<Vec<GE>>();

        // can run in parallel to g_vec:
        let h_vec = (0..nm)
            .map(|i| {
                let kzen_label_j = BigInt::from(n as u32) + BigInt::from(i as u32) + &kzen_label;
                let hash_j = HSha512::create_hash(&[&kzen_label_j]);
                generate_random_point(&Converter::to_vec(&hash_j))
            }).collect::<Vec<GE>>();

        let range = BigInt::from(2).pow(n as u32);
        let v_vec = (0..m)
            .map(|i| {
                let v = BigInt::sample_below(&range);
                let v_fe: FE = ECScalar::from(&v);
                v_fe
            }).collect::<Vec<FE>>();

        let r_vec = (0..m).map(|i| ECScalar::new_random()).collect::<Vec<FE>>();

        let ped_com_vec = (0..m)
            .map(|i| {
                let ped_com = G.clone() * &v_vec[i] + H.clone() * &r_vec[i];
                ped_com
            }).collect::<Vec<GE>>();

        let range_proof = RangeProof::prove(&g_vec, &h_vec, &G, &H, v_vec, r_vec, n);
        let result = RangeProof::verify(&range_proof, &g_vec, &h_vec, &G, &H, ped_com_vec, n);
        assert!(result.is_ok());
    }

    #[test]
    #[should_panic]
    pub fn test_batch_4_range_proof_32_out_of_range() {
        let n = 32;
        // num of proofs
        let m = 4;
        let nm = n * m;
        let KZen: &[u8] = &[75, 90, 101, 110];
        let kzen_label = BigInt::from(KZen);

        let G: GE = ECPoint::generator();
        let label = BigInt::from(1);
        let hash = HSha512::create_hash(&[&label]);
        let H = generate_random_point(&Converter::to_vec(&hash));

        let g_vec = (0..nm)
            .map(|i| {
                let kzen_label_i = BigInt::from(i as u32) + &kzen_label;
                let hash_i = HSha512::create_hash(&[&kzen_label_i]);
                generate_random_point(&Converter::to_vec(&hash_i))
            }).collect::<Vec<GE>>();

        // can run in parallel to g_vec:
        let h_vec = (0..nm)
            .map(|i| {
                let kzen_label_j = BigInt::from(n as u32) + BigInt::from(i as u32) + &kzen_label;
                let hash_j = HSha512::create_hash(&[&kzen_label_j]);
                generate_random_point(&Converter::to_vec(&hash_j))
            }).collect::<Vec<GE>>();

        let range = BigInt::from(2).pow(n as u32);
        let mut v_vec = (0..m - 1)
            .map(|i| {
                let v = BigInt::sample_below(&range);
                let v_fe: FE = ECScalar::from(&v);
                v_fe
            }).collect::<Vec<FE>>();

        let mut bad_v = BigInt::from(2).pow(33);
        v_vec.push(ECScalar::from(&bad_v));

        let r_vec = (0..m).map(|i| ECScalar::new_random()).collect::<Vec<FE>>();

        let ped_com_vec = (0..m)
            .map(|i| {
                let ped_com = G.clone() * &v_vec[i] + H.clone() * &r_vec[i];
                ped_com
            }).collect::<Vec<GE>>();

        let range_proof = RangeProof::prove(&g_vec, &h_vec, &G, &H, v_vec, r_vec, n);
        let result = RangeProof::verify(&range_proof, &g_vec, &h_vec, &G, &H, ped_com_vec, n);
        assert!(result.is_ok());
    }

    #[test]
    pub fn test_batch_2_range_proof_16() {
        let n = 16;
        // num of proofs
        let m = 2;
        let nm = n * m;
        let KZen: &[u8] = &[75, 90, 101, 110];
        let kzen_label = BigInt::from(KZen);

        let G: GE = ECPoint::generator();
        let label = BigInt::from(1);
        let hash = HSha512::create_hash(&[&label]);
        let H = generate_random_point(&Converter::to_vec(&hash));

        let g_vec = (0..nm)
            .map(|i| {
                let kzen_label_i = BigInt::from(i as u32) + &kzen_label;
                let hash_i = HSha512::create_hash(&[&kzen_label_i]);
                generate_random_point(&Converter::to_vec(&hash_i))
            }).collect::<Vec<GE>>();

        // can run in parallel to g_vec:
        let h_vec = (0..nm)
            .map(|i| {
                let kzen_label_j = BigInt::from(n as u32) + BigInt::from(i as u32) + &kzen_label;
                let hash_j = HSha512::create_hash(&[&kzen_label_j]);
                generate_random_point(&Converter::to_vec(&hash_j))
            }).collect::<Vec<GE>>();

        let range = BigInt::from(2).pow(n as u32);
        let v_vec = (0..m)
            .map(|i| {
                let v = BigInt::sample_below(&range);
                let v_fe: FE = ECScalar::from(&v);
                v_fe
            }).collect::<Vec<FE>>();

        let r_vec = (0..m).map(|i| ECScalar::new_random()).collect::<Vec<FE>>();

        let ped_com_vec = (0..m)
            .map(|i| {
                let ped_com = G.clone() * &v_vec[i] + H.clone() * &r_vec[i];
                ped_com
            }).collect::<Vec<GE>>();

        let range_proof = RangeProof::prove(&g_vec, &h_vec, &G, &H, v_vec, r_vec, n);
        let result = RangeProof::verify(&range_proof, &g_vec, &h_vec, &G, &H, ped_com_vec, n);
        assert!(result.is_ok());
    }

    #[test]
    pub fn test_batch_1_range_proof_8() {
        // bit range
        let n = 8;
        // batch size
        let m = 1;
        let nm = n * m;
        // some seed for generating g and h vectors
        let KZen: &[u8] = &[75, 90, 101, 110];
        let kzen_label = BigInt::from(KZen);

        // G,H - points for pederson commitment: com  = vG + rH
        let G: GE = ECPoint::generator();
        let label = BigInt::from(1);
        let hash = HSha512::create_hash(&[&label]);
        let H = generate_random_point(&Converter::to_vec(&hash));

        let g_vec = (0..nm)
            .map(|i| {
                let kzen_label_i = BigInt::from(i as u32) + &kzen_label;
                let hash_i = HSha512::create_hash(&[&kzen_label_i]);
                generate_random_point(&Converter::to_vec(&hash_i))
            }).collect::<Vec<GE>>();

        // can run in parallel to g_vec:
        let h_vec = (0..nm)
            .map(|i| {
                let kzen_label_j = BigInt::from(n as u32) + BigInt::from(i as u32) + &kzen_label;
                let hash_j = HSha512::create_hash(&[&kzen_label_j]);
                generate_random_point(&Converter::to_vec(&hash_j))
            }).collect::<Vec<GE>>();

        let range = BigInt::from(2).pow(n as u32);
        let v_vec = (0..m)
            .map(|i| {
                let v = BigInt::sample_below(&range);
                let v_fe: FE = ECScalar::from(&v);
                v_fe
            }).collect::<Vec<FE>>();

        let r_vec = (0..m).map(|i| ECScalar::new_random()).collect::<Vec<FE>>();

        let ped_com_vec = (0..m)
            .map(|i| {
                let ped_com = G.clone() * &v_vec[i] + H.clone() * &r_vec[i];
                ped_com
            }).collect::<Vec<GE>>();

        let range_proof = RangeProof::prove(&g_vec, &h_vec, &G, &H, v_vec, r_vec, n);
        let result = RangeProof::verify(&range_proof, &g_vec, &h_vec, &G, &H, ped_com_vec, n);
        assert!(result.is_ok());
    }
}