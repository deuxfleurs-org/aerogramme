    pub async fn fetch(
        &self,
        sequence_set: SequenceSet,
        attributes: MacroOrFetchAttributes,
        uid: bool,
    ) -> Result<Response> {
        Ok(vec![ImapRes::Status(
            Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
        )])
    }
