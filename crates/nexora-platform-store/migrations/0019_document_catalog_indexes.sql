CREATE INDEX project_documents_kind_page ON project_documents(project_id,kind,id);
CREATE INDEX document_publications_page ON document_publications(project_id,document_id,id);
