use std::{
    collections::BTreeSet,
    convert::{TryFrom, TryInto},
};
use types::{
    ContractPackage, ContractVersionKey, EntryPoint, EntryPointAccess, EntryPointType, Group,
    Parameter,
};

use crate::engine_server::{mappings::ParsingError, state};

impl From<ContractPackage> for state::ContractPackage {
    fn from(value: ContractPackage) -> state::ContractPackage {
        let mut metadata = state::ContractPackage::new();
        metadata.set_access_key(value.access_key().into());

        for &removed_version in value.removed_versions().iter() {
            metadata.mut_removed_versions().push(removed_version.into())
        }

        for (existing_group, urefs) in value.groups().iter() {
            let mut entrypoint_group = state::Contract_EntryPoint_Group::new();
            entrypoint_group.set_name(existing_group.value().to_string());

            let mut metadata_group = state::ContractPackage_Group::new();
            metadata_group.set_group(entrypoint_group);

            for &uref in urefs {
                metadata_group.mut_urefs().push(uref.into());
            }

            metadata.mut_groups().push(metadata_group);
        }

        for (version, contract_header) in value.take_versions().into_iter() {
            let mut active_version = state::ContractPackage_Version::new();
            active_version.set_version(version.into());
            active_version.set_contract_hash(contract_header.to_vec());
            metadata.mut_active_versions().push(active_version)
        }

        metadata
    }
}

impl TryFrom<state::ContractPackage> for ContractPackage {
    type Error = ParsingError;
    fn try_from(mut value: state::ContractPackage) -> Result<ContractPackage, Self::Error> {
        let access_uref = value.take_access_key().try_into()?;
        let mut contract_package = ContractPackage::new(access_uref);
        for mut active_version in value.take_active_versions().into_iter() {
            let version = active_version.take_version().try_into()?;
            let header = active_version.take_contract_hash().as_slice().try_into()?;
            contract_package.versions_mut().insert(version, header);
        }
        for removed_version in value.take_removed_versions().into_iter() {
            contract_package
                .removed_versions_mut()
                .insert(removed_version.try_into()?);
        }

        let groups = contract_package.groups_mut();
        for mut group in value.take_groups().into_iter() {
            let group_name = group.take_group().take_name();
            let mut urefs = BTreeSet::new();
            for uref in group.take_urefs().into_iter() {
                urefs.insert(uref.try_into()?);
            }
            groups.insert(Group::new(group_name), urefs);
        }
        Ok(contract_package)
    }
}

impl From<EntryPoint> for state::Contract_EntryPoint {
    fn from(value: EntryPoint) -> Self {
        let (name, args, ret, entry_point_access, entry_point_type) = value.into();

        let mut res = state::Contract_EntryPoint::new();
        res.set_name(name);

        for arg in args.into_iter() {
            let (name, cl_type) = arg.into();
            let mut state_arg = state::Contract_EntryPoint_Arg::new();

            state_arg.set_name(name);
            state_arg.set_cl_type(cl_type.into());

            res.mut_args().push(state_arg)
        }

        res.set_ret(ret.into());

        match entry_point_access {
            EntryPointAccess::Public => res.set_public(state::Contract_EntryPoint_Public::new()),
            EntryPointAccess::Groups(groups) => {
                let mut state_groups = state::Contract_EntryPoint_Groups::new();
                for group in groups.into_iter() {
                    let mut state_group = state::Contract_EntryPoint_Group::new();
                    let name = group.into();
                    state_group.set_name(name);
                    state_groups.mut_groups().push(state_group);
                }
                res.set_groups(state_groups)
            }
        }

        match entry_point_type {
            EntryPointType::Session => res.set_session(state::Contract_EntryPoint_Session::new()),
            EntryPointType::Contract => {
                res.set_contract(state::Contract_EntryPoint_Contract::new())
            }
        }
        res
    }
}

impl TryFrom<state::Contract_EntryPoint> for EntryPoint {
    type Error = ParsingError;
    fn try_from(mut value: state::Contract_EntryPoint) -> Result<EntryPoint, Self::Error> {
        let name = value.take_name();
        let mut args = Vec::new();

        let ret = value.take_ret().try_into()?;

        for mut arg in value.take_args().into_iter() {
            args.push(Parameter::new(
                arg.take_name(),
                arg.take_cl_type().try_into()?,
            ));
        }

        let entry_point_access = match value.access {
            Some(state::Contract_EntryPoint_oneof_access::public(_)) => EntryPointAccess::Public,
            Some(state::Contract_EntryPoint_oneof_access::groups(mut groups)) => {
                let mut vec = Vec::new();
                for mut group in groups.take_groups().into_iter() {
                    vec.push(Group::new(group.take_name()));
                }
                EntryPointAccess::Groups(vec)
            }
            None => return Err("Unable to parse Protobuf entry point access".into()),
        };
        let entry_point_type = match value.entry_point_type {
            Some(state::Contract_EntryPoint_oneof_entry_point_type::session(_)) => {
                EntryPointType::Session
            }
            Some(state::Contract_EntryPoint_oneof_entry_point_type::contract(_)) => {
                EntryPointType::Contract
            }
            None => return Err("Unable to parse Protobuf entry point type".into()),
        };
        Ok(EntryPoint::new(
            name,
            args,
            ret,
            entry_point_access,
            entry_point_type,
        ))
    }
}

impl From<ContractVersionKey> for state::ContractVersionKey {
    fn from(version: ContractVersionKey) -> Self {
        let mut contract_version_key = state::ContractVersionKey::new();
        contract_version_key.set_protocol_version_major(version.protocol_version_major());
        contract_version_key.set_contract_version(version.contract_version().into());
        contract_version_key
    }
}

impl TryFrom<state::ContractVersionKey> for ContractVersionKey {
    type Error = ParsingError;
    fn try_from(value: state::ContractVersionKey) -> Result<Self, Self::Error> {
        let contract_version = value
            .contract_version
            .try_into()
            .map_err(|_| ParsingError("Invalid value for contract version".to_string()))?;
        Ok(ContractVersionKey::new(
            value.protocol_version_major,
            contract_version,
        ))
    }
}
